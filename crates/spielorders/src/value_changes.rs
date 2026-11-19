//! Adding, renaming, merging, and deleting a Package's values, and merging and replacing
//! its Vendors: changes to the values `vendors.json` keeps that every Order using a changed
//! value follows.
//!
//! Each change is planned as the complete set of records it rewrites — the Vendor record
//! and every Order whose value changes — and committed as one guarded multi-file
//! replacement that restores what it wrote when a later write fails.
//!
//! Decision sources: Define the Order record and lifecycle contract; Define Vendor
//! identity and history; Durable managed-file writes.

use crate::{
    Answer, Failure,
    canonical::{
        CurrentPackageCandidate, ReusableValueProjection, Touched, load_every_record,
        load_package_to_change,
    },
    change::PlannedAct,
    integrity::{Aggregate, Holder, Records, Substitution},
    journal::{Expect, Journal, ManagedRecord, Subject},
    metadata::{
        NonBlankString, OrderDocument, StoredProject, StoredValues, Vendor, VendorId,
        VendorsDocument, Version1,
    },
    mutation::{CommitFailure, MutationReport, OperationKind},
    ports::FileSystem,
    readme::{Location, located_in_package},
    values::{ReusableValueKind, ValueScope, same_value},
    vendor_reads::VendorRead,
};
use std::path::{Path, PathBuf};

/// What a value change does to Orders that use a value it takes away.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InUse<T> {
    /// Refuse while any Order uses it.
    Refuse,
    /// Every such Order uses this existing value instead.
    Replace(T),
    /// Every such Order uses none.
    Clear,
}

/// One change to a Package's values or Vendors, as a caller names it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ValueRequest {
    Add {
        kind: ReusableValueKind,
        project: Option<String>,
        value: String,
    },
    /// Every Order using one of `values`, or the existing value `with` matches, uses
    /// `with`'s spelling; the values are kept as that one value. Where `existing`, `with`
    /// must name a value the Package keeps.
    Replace {
        kind: ReusableValueKind,
        project: Option<String>,
        values: Vec<String>,
        with: String,
        existing: bool,
    },
    Delete {
        kind: ReusableValueKind,
        project: Option<String>,
        value: String,
        in_use: InUse<String>,
    },
    /// Every Order referencing one of `vendors` references `keep`, which gains their
    /// Contacts; the others are deleted.
    MergeVendors { vendors: Vec<String>, keep: String },
    DeleteVendor {
        vendor: String,
        in_use: InUse<String>,
    },
}

/// One Order record a value change rewrites.
#[derive(Clone, Debug, PartialEq)]
struct OrderRewrite {
    path: PathBuf,
    prior: OrderDocument,
    candidate: OrderDocument,
}

/// The records one value change rewrites, as they stood when it was planned.
#[derive(Clone, Debug, PartialEq)]
struct Rewrites {
    prior: VendorsDocument,
    candidate: VendorsDocument,
    orders: Vec<OrderRewrite>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValueChangePlan {
    root: PathBuf,
    request: ValueRequest,
    vendors_path: PathBuf,
    rewrites: Rewrites,
    index: Location,
    pub(crate) journal: Journal,
}

impl ValueChangePlan {
    /// For a Vendor merge, the kept Vendor before the merge fills it and after; `None` for
    /// any other change.
    pub(crate) fn kept_vendor(&self) -> Option<[&Vendor; 2]> {
        let ValueRequest::MergeVendors { keep, .. } = &self.request else {
            return None;
        };
        fn find<'a>(document: &'a VendorsDocument, keep: &str) -> Option<&'a Vendor> {
            document
                .vendors
                .iter()
                .find(|vendor| vendor.vendor_id.as_str() == keep)
        }
        Some([
            find(&self.rewrites.prior, keep)?,
            find(&self.rewrites.candidate, keep)?,
        ])
    }

    /// Each Vendor this change removes from the Package.
    pub(crate) fn removed_vendors(&self) -> Vec<&Vendor> {
        self.rewrites
            .prior
            .vendors
            .iter()
            .filter(|vendor| {
                !self
                    .rewrites
                    .candidate
                    .vendors
                    .iter()
                    .any(|kept| kept.vendor_id == vendor.vendor_id)
            })
            .collect()
    }

    /// How many Order records committing this plan rewrites.
    pub fn order_count(&self) -> usize {
        self.rewrites.orders.len()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValueChangeResult {
    /// How many Orders the change rewrote.
    pub orders: usize,
    /// The affected kind's values after the change, scoped to a Project for groups.
    pub(crate) values: Option<Vec<ReusableValueProjection>>,
    /// The surviving Vendor after a merge or replacement deletion.
    pub(crate) vendor: Option<VendorRead>,
}

/// The Package as `request` must see it. Adding a value changes only the Vendor record;
/// every other change selects the Orders it rewrites by what their records hold, so it
/// reads every record.
fn loaded<F: FileSystem>(
    root: &Path,
    request: &ValueRequest,
    filesystem: &F,
) -> Result<CurrentPackageCandidate, Failure> {
    match request {
        ValueRequest::Add { .. } => {
            load_package_to_change(root, &Touched::default().with_vendors(true), filesystem)
        }
        _ => load_every_record(root, filesystem),
    }
}

pub(crate) fn plan_value_change<F: FileSystem>(
    selected_root: &Path,
    request: &ValueRequest,
    filesystem: &F,
) -> Answer<ValueChangePlan> {
    Answer::of(|| {
        let package = loaded(selected_root, request, filesystem)?;
        let rewrites = rewrites(request, &package, &records(request, &package, filesystem)?)?;
        let root = package.context.root.absolute_path.clone();
        let mut plan = ValueChangePlan {
            journal: Journal::new(&root),
            root,
            request: request.clone(),
            vendors_path: package.context.vendors.absolute_path.clone(),
            rewrites,
            index: package.context.index.clone(),
        };
        plan.journal = plan.value_journal(
            plan.records()
                .into_iter()
                .map(|(record, candidate)| {
                    let expect = record.expectation();
                    (record, candidate, expect)
                })
                .collect(),
        );
        Ok(plan)
    })
}

impl ValueChangePlan {
    /// Each record the change rewrites, as planned, with the bytes it becomes: every Order
    /// record, then the Vendor record.
    fn records(&self) -> Vec<(ManagedRecord, Vec<u8>)> {
        let vendors = (self.rewrites.candidate != self.rewrites.prior).then(|| {
            (
                ManagedRecord::vendors(self.vendors_path.clone(), self.rewrites.prior.clone()),
                self.rewrites.candidate.to_json_bytes().unwrap_or_default(),
            )
        });
        self.rewrites
            .orders
            .iter()
            .map(|order| {
                (
                    ManagedRecord::order(order.path.clone(), order.prior.clone()),
                    order.candidate.to_json_bytes().unwrap_or_default(),
                )
            })
            .chain(vendors)
            .collect()
    }

    /// The commit replacing every record in `records` together, each expected to still
    /// hold what its expectation names.
    fn value_journal(&self, records: Vec<(ManagedRecord, Vec<u8>, Expect)>) -> Journal {
        let mut journal = Journal::new(&self.root);
        if records.is_empty() {
            return journal;
        }
        for (record, candidate, expect) in records {
            let path = record.path().to_path_buf();
            let (kind, subject) = if path == self.vendors_path {
                (OperationKind::ReplaceVendorRecord, Subject::Vendors)
            } else {
                let directory = directory_of(&path).to_path_buf();
                (OperationKind::ReplaceOrderRecord, Subject::Order(directory))
            };
            journal.replace(path, candidate, expect, PlannedAct::Managed(kind), subject);
        }
        journal.refreshing(self.index.clone());
        journal
    }
}

pub(crate) fn commit_value_change<F: FileSystem>(
    plan: ValueChangePlan,
    filesystem: &F,
) -> Answer<MutationReport<ValueChangeResult>> {
    Answer::of(|| {
        let current = loaded(&plan.root, &plan.request, filesystem)?;
        let records = records(&plan.request, &current, filesystem)?;
        // The change planned again against the Package as it now stands must be the one
        // planned: the same records, holding the same facts, rewritten the same way.
        if rewrites(&plan.request, &current, &records).as_ref() != Ok(&plan.rewrites) {
            return Err(Failure::PreconditionsChanged {
                location: located_in_package(&plan.root, plan.root.clone()),
            });
        }
        let result = changed_result(&plan.request, &current, &plan.rewrites);
        let planned = plan.records();
        if planned.is_empty() {
            return Ok(MutationReport::Unchanged { result });
        }
        // Every candidate is a whole Order record before anything is written.
        for order in &plan.rewrites.orders {
            if order.candidate.validate().is_err() {
                return Err(Failure::commit(
                    CommitFailure::new(located_in_package(&plan.root, order.path.clone())),
                    Vec::new(),
                ));
            }
        }
        // Each record is read once more and must still mean what was planned against; the
        // bytes read are what its replacement then expects, so a writer landing after
        // this read is refused rather than overwritten.
        let mut records = Vec::new();
        for (record, candidate) in planned {
            let observed = record.confirm(&plan.root, filesystem)?;
            records.push((record, candidate, Expect::Bytes(observed)));
        }
        let committed = plan.value_journal(records).run(filesystem)?;
        let refresh = committed.refresh(&plan.root, filesystem);
        Ok(MutationReport::committed(
            result,
            committed.warnings(refresh),
        ))
    })
}

/// The immutable affected result that the validated replacements will produce.
fn changed_result(
    request: &ValueRequest,
    package: &CurrentPackageCandidate,
    rewrites: &Rewrites,
) -> ValueChangeResult {
    let documents = rewritten_orders(package, &rewrites.orders);
    let projected = crate::values::project(&rewrites.candidate.values, documents.iter());
    let values = match request {
        ValueRequest::Add { kind, project, .. }
        | ValueRequest::Replace { kind, project, .. }
        | ValueRequest::Delete { kind, project, .. } => Some(
            ValueScope::new(*kind, project.as_deref())
                .listed(&projected)
                .to_vec(),
        ),
        _ => None,
    };
    let survivor = match request {
        ValueRequest::MergeVendors { keep, .. } => Some(keep),
        ValueRequest::DeleteVendor {
            in_use: InUse::Replace(replacement),
            ..
        } => Some(replacement),
        _ => None,
    };
    let vendor = survivor.and_then(|id| {
        rewrites
            .candidate
            .vendors
            .iter()
            .find(|vendor| vendor.vendor_id.as_str() == id)
            .map(|vendor| VendorRead {
                context: package.context.clone(),
                vendor: vendor.clone(),
                diagnostics: package.findings(),
            })
    });
    ValueChangeResult {
        orders: rewrites.orders.len(),
        values,
        vendor,
    }
}

fn rewritten_orders(
    package: &CurrentPackageCandidate,
    rewrites: &[OrderRewrite],
) -> Vec<OrderDocument> {
    let changed = rewrites
        .iter()
        .map(|rewrite| (rewrite.path.as_path(), &rewrite.candidate))
        .collect::<std::collections::BTreeMap<_, _>>();
    package
        .healthy_orders
        .iter()
        .map(|order| {
            changed
                .get(order.paths.order.absolute_path.as_path())
                .copied()
                .unwrap_or(&order.document)
                .clone()
        })
        .collect()
}

/// The records `request` must account for: a change to which Vendor Orders reference
/// accounts for the removed Orders too, which undoing a removal restores as they were.
fn records<'p, F: FileSystem>(
    request: &ValueRequest,
    package: &'p CurrentPackageCandidate,
    filesystem: &F,
) -> Result<Records<'p>, Failure> {
    let records = Records::of(package);
    match request {
        ValueRequest::MergeVendors { .. } | ValueRequest::DeleteVendor { .. } => {
            records.with_removed(&package.context, filesystem)
        }
        _ => Ok(records),
    }
}

/// The records `request` rewrites in `package`, or why it is refused.
fn rewrites(
    request: &ValueRequest,
    package: &CurrentPackageCandidate,
    records: &Records,
) -> Result<Rewrites, Failure> {
    let prior = VendorsDocument {
        schema_version: Version1,
        vendors: package.vendors.clone(),
        values: package.values.clone(),
    };
    let mut candidate = prior.clone();
    let mut orders = Vec::new();
    match request {
        ValueRequest::Add {
            kind,
            project,
            value,
        } => {
            let scope = ValueScope::of(*kind, project.as_deref(), package)?;
            let value = nonblank(scope.field(), value)?;
            if scope.existing(package, &value).is_none() {
                scope.keep(&mut candidate.values, &value);
            }
        }
        ValueRequest::Replace {
            kind,
            project,
            values,
            with,
            existing,
        } => {
            let scope = ValueScope::of(*kind, project.as_deref(), package)?;
            if *existing {
                scope.known(package, with)?;
            }
            let with = nonblank(scope.field(), with)?;
            let mut taken = values
                .iter()
                .map(|value| scope.known(package, value))
                .collect::<Result<Vec<_>, _>>()?;
            taken.extend(scope.existing(package, &with));
            replace(
                &scope,
                package,
                &taken,
                Some(&with),
                &mut candidate.values,
                &mut orders,
            );
        }
        ValueRequest::Delete {
            kind,
            project,
            value,
            in_use,
        } => {
            let scope = ValueScope::of(*kind, project.as_deref(), package)?;
            let value = scope.known(package, value)?;
            let with = match in_use {
                InUse::Refuse => {
                    let users = scope.users(package, std::slice::from_ref(&value));
                    if !users.is_empty() {
                        return Err(Failure::ValueInUse {
                            field: scope.field().to_owned(),
                            value,
                            orders: users
                                .into_iter()
                                .map(|order| package.healthy_orders[order].paths.directory.clone())
                                .collect(),
                        });
                    }
                    None
                }
                InUse::Replace(replacement) => {
                    let replacement = scope.known(package, replacement)?;
                    if same_value(&replacement, &value) {
                        return Err(Failure::InvalidFacts);
                    }
                    Some(replacement)
                }
                InUse::Clear => None,
            };
            replace(
                &scope,
                package,
                &[value],
                with.as_deref(),
                &mut candidate.values,
                &mut orders,
            );
        }
        ValueRequest::MergeVendors { vendors, keep } => {
            let keep = vendor_position(package, keep)?;
            let mut merged = Vec::new();
            for vendor in vendors {
                let position = vendor_position(package, vendor)?;
                if position != keep && !merged.contains(&position) {
                    merged.push(position);
                }
            }
            let kept_id = package.vendors[keep].vendor_id.clone();
            let mut survivor = package.vendors[keep].clone();
            // The kept Vendor takes precedence, and each merged one after it only fills what
            // the result still lacks, in the order named.
            for position in &merged {
                fill_vendor(&mut survivor, &package.vendors[*position]);
            }
            let merged_ids = merged
                .iter()
                .map(|position| package.vendors[*position].vendor_id.clone())
                .collect::<Vec<_>>();
            for vendor in &merged_ids {
                refuse_fixed_references(records, vendor)?;
            }
            candidate.vendors = package
                .vendors
                .iter()
                .filter(|vendor| !merged_ids.contains(&vendor.vendor_id))
                .map(|vendor| {
                    if vendor.vendor_id == kept_id {
                        survivor.clone()
                    } else {
                        vendor.clone()
                    }
                })
                .collect();
            repoint(records, &merged_ids, Some(&kept_id), &mut orders);
        }
        ValueRequest::DeleteVendor { vendor, in_use } => {
            let position = vendor_position(package, vendor)?;
            let deleted = package.vendors[position].vendor_id.clone();
            refuse_fixed_references(records, &deleted)?;
            let replacement = match in_use {
                InUse::Refuse => return Err(Failure::InvalidFacts),
                InUse::Replace(replacement) => {
                    let replacement = vendor_position(package, replacement)?;
                    if replacement == position {
                        return Err(Failure::InvalidFacts);
                    }
                    Some(package.vendors[replacement].vendor_id.clone())
                }
                InUse::Clear => None,
            };
            candidate.vendors.remove(position);
            repoint(
                records,
                std::slice::from_ref(&deleted),
                replacement.as_ref(),
                &mut orders,
            );
        }
    }
    let aggregate = Aggregate::judge(&candidate).map_err(|_| Failure::InvalidAggregate)?;
    let substitution = Substitution {
        orders: orders
            .iter()
            .map(|order| (directory_of(&order.path), &order.candidate))
            .collect(),
        vendors: Some(&aggregate),
    };
    if orders
        .iter()
        .any(|order| order.candidate.validate().is_err())
        || records.broken_by(&substitution).is_some()
    {
        return Err(Failure::InvalidAggregate);
    }
    let candidate = aggregate.into_document();
    Ok(Rewrites {
        prior,
        candidate,
        orders,
    })
}

/// The Order Directory of an Order record.
fn directory_of(record: &Path) -> &Path {
    record
        .parent()
        .and_then(Path::parent)
        .expect("an Order record lies inside its Order Directory's metadata directory")
}

/// Refuses taking away a Vendor that an Order this change cannot rewrite still names.
fn refuse_fixed_references(records: &Records, vendor: &VendorId) -> Result<(), Failure> {
    let orders = records
        .naming(std::slice::from_ref(vendor))
        .into_iter()
        .filter(|holder| matches!(holder, Holder::Fixed { .. }))
        .map(|holder| holder.directory().clone())
        .collect::<Vec<_>>();
    if orders.is_empty() {
        Ok(())
    } else {
        Err(Failure::VendorReferenced {
            vendor_id: vendor.as_str().to_owned(),
            orders,
        })
    }
}

/// Fills each fact and Contact `kept` lacks from `merged`: an absent fact takes
/// `merged`'s, a Contact `kept` has by Name gains the Contact facts it lacks, and any other
/// Contact is added after `kept`'s own.
fn fill_vendor(kept: &mut Vendor, merged: &Vendor) {
    fn fill(kept: &mut Option<String>, merged: &Option<String>) {
        if kept.is_none() {
            kept.clone_from(merged);
        }
    }
    fill(&mut kept.phone, &merged.phone);
    fill(&mut kept.email, &merged.email);
    fill(&mut kept.website, &merged.website);
    fill(&mut kept.address, &merged.address);
    fill(&mut kept.purchasing_notes, &merged.purchasing_notes);
    for contact in &merged.contacts {
        match kept.contacts.iter_mut().find(|own| {
            crate::vendors::vendor_names_match(own.name.as_str(), contact.name.as_str())
        }) {
            Some(own) => {
                fill(&mut own.role, &contact.role);
                fill(&mut own.phone, &contact.phone);
                fill(&mut own.email, &contact.email);
            }
            None => kept.contacts.push(contact.clone()),
        }
    }
}

/// The position of the Vendor with this ID.
fn vendor_position(package: &CurrentPackageCandidate, vendor_id: &str) -> Result<usize, Failure> {
    package
        .vendors
        .iter()
        .position(|vendor| vendor.vendor_id.as_str() == vendor_id)
        .ok_or_else(|| Failure::UnknownVendorId {
            vendor_id: vendor_id.to_owned(),
        })
}

/// Every Order referencing a Vendor in `from`, active or removed, rewritten to reference
/// `to`, or none.
fn repoint(
    records: &Records,
    from: &[VendorId],
    to: Option<&VendorId>,
    orders: &mut Vec<OrderRewrite>,
) {
    for holder in records.naming(from) {
        if let Holder::Rewritable { record, order, .. } = holder {
            let mut candidate = order.clone();
            candidate.vendor_id = to.cloned();
            orders.push(OrderRewrite {
                path: record.to_path_buf(),
                prior: order.clone(),
                candidate,
            });
        }
    }
}

fn nonblank(field: &str, value: &str) -> Result<String, Failure> {
    NonBlankString::trimmed(value)
        .map(|value| value.as_str().to_owned())
        .ok_or_else(|| Failure::BlankValue {
            field: field.to_owned(),
        })
}

/// Takes `taken` out of `scope`, keeping `with` in their place, and has every Order
/// using one of them use `with`, or none. A Project's kept groups go with it.
fn replace(
    scope: &ValueScope,
    package: &CurrentPackageCandidate,
    taken: &[String],
    with: Option<&str>,
    values: &mut StoredValues,
    orders: &mut Vec<OrderRewrite>,
) {
    let is_taken = |text: &str| taken.iter().any(|value| same_value(text, value));
    // Where Projects merge, the Order Groups they now share, in the spelling each keeps.
    let mut merged_groups = Vec::<NonBlankString>::new();
    let without = |list: &mut Vec<NonBlankString>| list.retain(|kept| !is_taken(kept.as_str()));
    match scope {
        ValueScope::OrderingInstitution => without(&mut values.ordering_institutions),
        ValueScope::Orderer => without(&mut values.orderers),
        ValueScope::OrderGroup { project: None } => without(&mut values.order_groups),
        ValueScope::OrderGroup {
            project: Some(project),
        } => {
            for kept in &mut values.projects {
                if same_value(kept.name.as_str(), project) {
                    without(&mut kept.order_groups);
                }
            }
        }
        ValueScope::Project => {
            // The kept Project's groups come first, then each other's in the order
            // named, so a group two Projects spell differently keeps the kept
            // Project's spelling.
            let mut groups = Vec::new();
            for value in with.into_iter().chain(taken.iter().map(String::as_str)) {
                for kept in &values.projects {
                    if same_value(kept.name.as_str(), value) {
                        groups.extend(kept.order_groups.iter().cloned());
                    }
                }
            }
            values.projects.retain(|kept| !is_taken(kept.name.as_str()));
            match with {
                Some(with) => {
                    merged_groups.clone_from(&groups);
                    values.projects.push(StoredProject {
                        name: NonBlankString::trimmed(with).expect("a replacement is nonblank"),
                        order_groups: groups,
                    });
                }
                None => {
                    values.order_groups.extend(groups);
                    merged_groups.clone_from(&values.order_groups);
                }
            }
        }
    }
    if let Some(with) = with
        && !matches!(scope, ValueScope::Project)
    {
        scope.keep(values, with);
    }
    for position in scope.users(package, taken) {
        let order = &package.healthy_orders[position];
        let mut candidate = order.document.clone();
        scope.set(&mut candidate, with);
        if let Some(group) = &mut candidate.order_group
            && let Some(kept) = merged_groups
                .iter()
                .find(|kept| same_value(kept.as_str(), group))
        {
            kept.as_str().clone_into(group);
        }
        if candidate == order.document {
            continue;
        }
        orders.push(OrderRewrite {
            path: order.paths.order.absolute_path.clone(),
            prior: order.document.clone(),
            candidate,
        });
    }
}
