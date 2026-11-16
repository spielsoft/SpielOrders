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
    filesystem::{FileReplacement, replace_validated_files},
    metadata::{
        NonBlankString, OrderDocument, StoredProject, StoredValues, Vendor, VendorId,
        VendorsDocument, Version1,
    },
    mutation::{IndexRefresh, MutationReport, OperationKind, PlannedOperation},
    package::PackageMetadata,
    ports::FileSystem,
    readme::located_in_package,
    search::ReusableValueKind,
    values::same_value,
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
    /// `with`'s spelling; the values are kept as that one value.
    Replace {
        kind: ReusableValueKind,
        project: Option<String>,
        values: Vec<String>,
        with: String,
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
    pub(crate) operations: Vec<PlannedOperation>,
}

impl ValueChangePlan {
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
        let rewrites = rewrites(request, &package)?;
        let mut operations = rewrites
            .orders
            .iter()
            .map(|order| {
                PlannedOperation::new(
                    OperationKind::ReplaceOrderRecord,
                    located_in_package(&package.context.root.absolute_path, order.path.clone()),
                )
            })
            .collect::<Vec<_>>();
        if rewrites.candidate != rewrites.prior {
            operations.push(PlannedOperation::new(
                OperationKind::ReplaceVendorRecord,
                package.context.vendors.clone(),
            ));
        }
        if !operations.is_empty() {
            operations.push(PlannedOperation::new(
                OperationKind::RefreshPackageIndex,
                package.context.index.clone(),
            ));
        }
        Ok(ValueChangePlan {
            root: package.context.root.absolute_path.clone(),
            request: request.clone(),
            vendors_path: package.context.vendors.absolute_path.clone(),
            rewrites,
            operations,
        })
    })
}

pub(crate) fn commit_value_change<F: FileSystem>(
    plan: ValueChangePlan,
    filesystem: &F,
) -> Answer<MutationReport<ValueChangeResult>> {
    Answer::of(|| {
        let current = loaded(&plan.root, &plan.request, filesystem)?;
        // The change planned again against the Package as it now stands must be the one
        // planned: the same records, holding the same facts, rewritten the same way.
        if rewrites(&plan.request, &current).as_ref() != Ok(&plan.rewrites) {
            return Err(Failure::PreconditionsChanged {
                location: located_in_package(&plan.root, plan.root.clone()),
            });
        }
        let result = changed_result(&plan.request, &current, &plan.rewrites);
        if plan.operations.is_empty() {
            return Ok(MutationReport::Unchanged { result });
        }
        // Each record is read once more and must still mean what was planned against; the
        // bytes read are what its replacement then expects, so a writer landing after
        // this read is refused rather than overwritten.
        let changed = |path: &Path| Failure::PreconditionsChanged {
            location: located_in_package(&plan.root, path.to_path_buf()),
        };
        let mut candidates = Vec::new();
        for order in &plan.rewrites.orders {
            let observed = filesystem
                .read(&order.path)
                .map_err(|_| changed(&order.path))?;
            let holds_prior = OrderDocument::from_json_bytes(&observed)
                .ok()
                .and_then(|document| document.validate().ok())
                .is_some_and(|document| document.order() == &order.prior);
            if !holds_prior {
                return Err(changed(&order.path));
            }
            let candidate = order
                .candidate
                .to_json_bytes()
                .map_err(|_| Failure::InvalidFacts)?;
            candidates.push((order.path.clone(), candidate, observed));
        }
        if plan.rewrites.candidate != plan.rewrites.prior {
            let path = &plan.vendors_path;
            let observed = filesystem.read(path).map_err(|_| changed(path))?;
            let holds_prior = VendorsDocument::from_json_bytes(&observed)
                .ok()
                .and_then(|document| PackageMetadata::new(&[], &document).validate().ok())
                .is_some_and(|projected| projected.vendors() == &plan.rewrites.prior);
            if !holds_prior {
                return Err(changed(path));
            }
            let candidate = plan
                .rewrites
                .candidate
                .to_json_bytes()
                .map_err(|_| Failure::InvalidAggregate)?;
            candidates.push((path.clone(), candidate, observed));
        }
        let replacements = candidates
            .iter()
            .map(|(path, candidate, observed)| FileReplacement::observed(path, candidate, observed))
            .collect::<Vec<_>>();
        let vendors_path = plan.vendors_path.as_path();
        if let Err(error) =
            replace_validated_files(&replacements, filesystem, |destination, candidate| {
                if destination == vendors_path {
                    // Judged as a Package aggregate while planning, again just above.
                    Ok(())
                } else {
                    OrderDocument::from_json_bytes(candidate)
                        .ok()
                        .and_then(|document| document.validate().ok())
                        .map(|_| ())
                        .ok_or(())
                }
            })
        {
            let destinations = replacements
                .iter()
                .map(FileReplacement::destination)
                .collect::<Vec<_>>();
            return Err(Failure::of_replacements(&plan.root, &destinations, error));
        }
        let touched = Touched::orders(
            plan.rewrites
                .orders
                .iter()
                .filter_map(|order| Some(order.path.parent()?.parent()?.to_path_buf())),
        )
        .with_vendors(plan.rewrites.candidate != plan.rewrites.prior);
        Ok(IndexRefresh::after_commit(&plan.root, &touched, filesystem).report(result))
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
        | ValueRequest::Delete { kind, project, .. } => Some(match kind {
            ReusableValueKind::OrderingInstitution => projected.ordering_institutions,
            ReusableValueKind::Orderer => projected.orderers,
            ReusableValueKind::Project => projected.projects,
            ReusableValueKind::OrderGroup => projected.order_groups_of(project.as_deref()).to_vec(),
        }),
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

/// The records `request` rewrites in `package`, or why it is refused.
fn rewrites(
    request: &ValueRequest,
    package: &CurrentPackageCandidate,
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
            let scope = Scope::of(*kind, project.as_deref(), package)?;
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
        } => {
            let scope = Scope::of(*kind, project.as_deref(), package)?;
            let with = nonblank(scope.field(), with)?;
            let mut taken = values
                .iter()
                .map(|value| scope.known(package, value))
                .collect::<Result<Vec<_>, _>>()?;
            taken.extend(scope.existing(package, &with));
            scope.replace(
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
            let scope = Scope::of(*kind, project.as_deref(), package)?;
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
            scope.replace(
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
                refuse_damaged_references(package, vendor)?;
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
            repoint(package, &merged_ids, Some(&kept_id), &mut orders);
        }
        ValueRequest::DeleteVendor { vendor, in_use } => {
            let position = vendor_position(package, vendor)?;
            let deleted = package.vendors[position].vendor_id.clone();
            refuse_damaged_references(package, &deleted)?;
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
                package,
                std::slice::from_ref(&deleted),
                replacement.as_ref(),
                &mut orders,
            );
        }
    }
    crate::values::normalize(&mut candidate.values);
    let documents = rewritten_orders(package, &orders);
    let candidate = PackageMetadata::new(&documents, &candidate)
        .validate()
        .map_err(|_| Failure::InvalidAggregate)?
        .vendors()
        .clone();
    Ok(Rewrites {
        prior,
        candidate,
        orders,
    })
}

fn refuse_damaged_references(
    package: &CurrentPackageCandidate,
    vendor: &VendorId,
) -> Result<(), Failure> {
    let orders = crate::vendors::damaged_references(package, vendor.as_str());
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

/// Has every Order referencing one of `from` reference `to`, or none.
fn repoint(
    package: &CurrentPackageCandidate,
    from: &[VendorId],
    to: Option<&VendorId>,
    orders: &mut Vec<OrderRewrite>,
) {
    for order in &package.healthy_orders {
        if order
            .document
            .vendor_id
            .as_ref()
            .is_some_and(|id| from.contains(id))
        {
            let mut candidate = order.document.clone();
            candidate.vendor_id = to.cloned();
            orders.push(OrderRewrite {
                path: order.paths.order.absolute_path.clone(),
                prior: order.document.clone(),
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

/// Where one kind of value lives: a list of its own, or the Order Groups of one Project or
/// of the Orders with no Project.
#[derive(Clone, Debug)]
enum Scope {
    OrderingInstitution,
    Orderer,
    Project,
    OrderGroup { project: Option<String> },
}

impl Scope {
    fn of(
        kind: ReusableValueKind,
        project: Option<&str>,
        package: &CurrentPackageCandidate,
    ) -> Result<Self, Failure> {
        Ok(match kind {
            ReusableValueKind::OrderingInstitution => Self::OrderingInstitution,
            ReusableValueKind::Orderer => Self::Orderer,
            ReusableValueKind::Project => Self::Project,
            ReusableValueKind::OrderGroup => Self::OrderGroup {
                project: match project.map(str::trim).filter(|project| !project.is_empty()) {
                    None => None,
                    Some(project) => Some(Self::Project.known(package, project)?),
                },
            },
        })
    }

    fn field(&self) -> &'static str {
        match self {
            Self::OrderingInstitution => "ordering_institution",
            Self::Orderer => "orderer",
            Self::Project => "project",
            Self::OrderGroup { .. } => "order_group",
        }
    }

    /// The Package's spelling of `value` in this scope, where it has one.
    fn existing(&self, package: &CurrentPackageCandidate, value: &str) -> Option<String> {
        let projections = &package.projections;
        let values = match self {
            Self::OrderingInstitution => &projections.ordering_institutions[..],
            Self::Orderer => &projections.orderers[..],
            Self::Project => &projections.projects[..],
            Self::OrderGroup { project } => projections.order_groups_of(project.as_deref()),
        };
        values
            .iter()
            .find(|existing| same_value(&existing.value, value))
            .map(|existing| existing.value.clone())
    }

    /// The Package's spelling of `value`, or the refusal naming it as unknown.
    fn known(&self, package: &CurrentPackageCandidate, value: &str) -> Result<String, Failure> {
        self.existing(package, value)
            .ok_or_else(|| Failure::UnknownValue {
                field: self.field().to_owned(),
                value: value.trim().to_owned(),
            })
    }

    /// The text of this scope's value on `order`, where the Order is in this scope.
    fn value_of<'a>(&self, order: &'a OrderDocument) -> Option<&'a str> {
        match self {
            Self::OrderingInstitution => order.ordering_institution.as_deref(),
            Self::Orderer => order.orderer.as_deref(),
            Self::Project => order.project.as_deref(),
            Self::OrderGroup { project } => {
                let own = order
                    .project
                    .as_deref()
                    .map(str::trim)
                    .filter(|own| !own.is_empty());
                let in_scope = match (own, project) {
                    (None, None) => true,
                    (Some(own), Some(project)) => same_value(own, project),
                    _ => false,
                };
                in_scope.then_some(order.order_group.as_deref()).flatten()
            }
        }
    }

    fn set(&self, order: &mut OrderDocument, value: Option<&str>) {
        let value = value.map(str::to_owned);
        match self {
            Self::OrderingInstitution => order.ordering_institution = value,
            Self::Orderer => order.orderer = value,
            Self::Project => order.project = value,
            Self::OrderGroup { .. } => order.order_group = value,
        }
    }

    /// The positions of the healthy Orders using one of `values`.
    fn users(&self, package: &CurrentPackageCandidate, values: &[String]) -> Vec<usize> {
        package
            .healthy_orders
            .iter()
            .enumerate()
            .filter(|(_, order)| {
                self.value_of(&order.document)
                    .is_some_and(|own| values.iter().any(|value| same_value(own, value)))
            })
            .map(|(position, _)| position)
            .collect()
    }

    /// Keeps `value` in this scope.
    fn keep(&self, values: &mut StoredValues, value: &str) {
        let Some(value) = NonBlankString::trimmed(value) else {
            return;
        };
        match self {
            Self::OrderingInstitution => values.ordering_institutions.push(value),
            Self::Orderer => values.orderers.push(value),
            Self::Project => values.projects.push(StoredProject {
                name: value,
                order_groups: Vec::new(),
            }),
            Self::OrderGroup { project: None } => values.order_groups.push(value),
            Self::OrderGroup {
                project: Some(project),
            } => match values
                .projects
                .iter_mut()
                .find(|kept| same_value(kept.name.as_str(), project))
            {
                Some(kept) => kept.order_groups.push(value),
                None => values.projects.push(StoredProject {
                    name: NonBlankString::trimmed(project).expect("a known Project is nonblank"),
                    order_groups: vec![value],
                }),
            },
        }
    }

    /// Takes `taken` out of this scope, keeping `with` in their place, and has every Order
    /// using one of them use `with`, or none. A Project's kept groups go with it.
    fn replace(
        &self,
        package: &CurrentPackageCandidate,
        taken: &[String],
        with: Option<&str>,
        values: &mut StoredValues,
        orders: &mut Vec<OrderRewrite>,
    ) {
        let is_taken = |text: &str| taken.iter().any(|value| same_value(text, value));
        let without = |list: &mut Vec<NonBlankString>| list.retain(|kept| !is_taken(kept.as_str()));
        match self {
            Self::OrderingInstitution => without(&mut values.ordering_institutions),
            Self::Orderer => without(&mut values.orderers),
            Self::OrderGroup { project: None } => without(&mut values.order_groups),
            Self::OrderGroup {
                project: Some(project),
            } => {
                for kept in &mut values.projects {
                    if same_value(kept.name.as_str(), project) {
                        without(&mut kept.order_groups);
                    }
                }
            }
            Self::Project => {
                let mut groups = Vec::new();
                values.projects.retain(|kept| {
                    let taken = is_taken(kept.name.as_str());
                    if taken {
                        groups.extend(kept.order_groups.iter().cloned());
                    }
                    !taken
                });
                match with {
                    Some(with) => values.projects.push(StoredProject {
                        name: NonBlankString::trimmed(with).expect("a replacement is nonblank"),
                        order_groups: groups,
                    }),
                    None => values.order_groups.extend(groups),
                }
            }
        }
        if let Some(with) = with
            && !matches!(self, Self::Project)
        {
            self.keep(values, with);
        }
        for position in self.users(package, taken) {
            let order = &package.healthy_orders[position];
            if self.value_of(&order.document) == with {
                continue;
            }
            let mut candidate = order.document.clone();
            self.set(&mut candidate, with);
            orders.push(OrderRewrite {
                path: order.paths.order.absolute_path.clone(),
                prior: order.document.clone(),
                candidate,
            });
        }
    }
}
