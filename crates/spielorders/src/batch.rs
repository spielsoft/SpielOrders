//! Many record changes as one guarded write: each change is planned against the Package
//! as the changes before it leave it, in order, and the whole batch commits every record
//! it changed together, or nothing.
//!
//! Decision sources: Define the Rust CLI capability contract; Durable managed-file writes.

use crate::{
    Answer, Change, Failure, OrderRef,
    canonical::{CurrentPackageCandidate, Touched, load_package_to_change},
    change::PlannedAct,
    integrity::{Aggregate, Records, Substitution},
    journal::{Expect, Journal, ManagedRecord, Subject},
    metadata::{OrderDocument, VendorId, VendorsDocument, Version1},
    mutation::{CommitFailure, MutationReport, OperationKind},
    order_edits::edited_order,
    ports::{FileSystem, FileSystemEntryKind, LocalDate},
    readme::Location,
    readme::located_in_package,
    vendors::{edited_vendor, vendor_creation_in},
};
use std::path::{Path, PathBuf};

/// A batch of record changes, holding the Package as its changes so far leave it.
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    initial: CurrentPackageCandidate,
    current: CurrentPackageCandidate,
    changes: usize,
}

impl Batch {
    /// An empty batch over `package` as loaded.
    pub(crate) fn new(package: CurrentPackageCandidate) -> Self {
        Self {
            current: package.clone(),
            initial: package,
            changes: 0,
        }
    }

    /// The Package as the batch's changes so far leave it.
    pub(crate) fn package(&self) -> &CurrentPackageCandidate {
        &self.current
    }

    /// How many changes the batch holds.
    pub fn len(&self) -> usize {
        self.changes
    }

    /// Plans `change` against the Package as the batch leaves it and, where it is
    /// accepted, applies it there, answering that change's own plan; a refused change
    /// leaves the batch as it was.
    ///
    /// A batch holds edits to existing Orders and Vendors and new Vendors, whose Vendor
    /// IDs `new_uuid` mints now so that later changes can name them. The first change to
    /// an Order starts from its record on disk, which must hold what the batch holds.
    pub(crate) fn add<F: FileSystem>(
        &mut self,
        change: &Change,
        today: &dyn Fn() -> LocalDate,
        new_uuid: &dyn Fn() -> String,
        filesystem: &F,
    ) -> Result<crate::Plan, Failure> {
        let package = &self.current;
        let planned = match change {
            Change::SaveOrder {
                order: OrderRef::Directory(directory),
                assignments,
                items,
                original,
            } => {
                let position = package
                    .healthy_orders
                    .iter()
                    .position(|order| order.paths.directory.absolute_path == *directory)
                    .ok_or_else(|| changed(package, directory))?;
                let order = &package.healthy_orders[position];
                if order.document != *original {
                    return Err(changed(package, directory));
                }
                let unchanged = order.document == self.initial.healthy_orders[position].document;
                if unchanged && !holds(&order.paths.directory.absolute_path, original, filesystem) {
                    return Err(changed(package, directory));
                }
                let plan = edited_order(package, order, assignments, items, today)
                    .map_err(Failure::InvalidDraft)?;
                let replacement = order
                    .with_document(&plan.facts.document)
                    .ok_or(Failure::InvalidFacts)?;
                let values = crate::values::merged(&package.values, plan.additions());
                self.current.replace_order(position, replacement, values);
                crate::Plan::of(plan)
            }
            Change::CreateVendor {
                assignments,
                contacts,
                ..
            } => {
                let plan = vendor_creation_in(package, assignments, contacts)?;
                let generated = new_uuid();
                let identity = serde_json::from_value::<VendorId>(serde_json::Value::String(
                    generated.clone(),
                ))
                .map_err(|_| Failure::InvalidGeneratedIdentity {
                    generated: generated.clone(),
                })?;
                if package
                    .vendors
                    .iter()
                    .any(|vendor| vendor.vendor_id == identity)
                {
                    return Err(Failure::GeneratedVendorIdentityCollision {
                        vendor_id: generated,
                    });
                }
                let mut vendors = plan.candidate_vendors().to_vec();
                for vendor in &mut vendors {
                    if vendor.vendor_id.as_str() == crate::mutation::PLANNING_IDENTITY {
                        vendor.vendor_id = identity.clone();
                    }
                }
                self.current.replace_vendors(vendors);
                crate::Plan::of(plan)
            }
            Change::SaveVendor {
                assignments,
                contacts,
                original,
                ..
            } => {
                let position = package
                    .vendors
                    .iter()
                    .position(|vendor| vendor.vendor_id == original.vendor_id)
                    .ok_or_else(|| Failure::UnknownVendorId {
                        vendor_id: original.vendor_id.as_str().to_owned(),
                    })?;
                if package.vendors[position] != *original {
                    return Err(Failure::PreconditionsChanged {
                        location: package.context.vendors.clone(),
                    });
                }
                let plan = edited_vendor(package, position, assignments, Some(contacts))
                    .map_err(Failure::InvalidDraft)?;
                let vendors = plan.candidate_vendors().to_vec();
                self.current.replace_vendors(vendors);
                crate::Plan::of(plan)
            }
            _ => return Err(Failure::NotBatchable),
        };
        self.changes += 1;
        Ok(planned)
    }
}

/// Whether this ordinary Order's own record holds `order`, as a load would project it.
fn holds<F: FileSystem>(directory: &Path, order: &OrderDocument, filesystem: &F) -> bool {
    let metadata = directory.join(".spielorders");
    let record = metadata.join("order.json");
    for (path, expected) in [
        (directory, FileSystemEntryKind::Directory),
        (metadata.as_path(), FileSystemEntryKind::Directory),
        (record.as_path(), FileSystemEntryKind::File),
    ] {
        if filesystem.symlink_metadata(path).ok() != Some(Some(expected)) {
            return false;
        }
    }
    filesystem
        .read(&record)
        .ok()
        .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
        .and_then(|document| document.validate().ok())
        .is_some_and(|document| document.order() == order)
}

fn changed(package: &CurrentPackageCandidate, path: &Path) -> Failure {
    Failure::PreconditionsChanged {
        location: located_in_package(&package.context.root.absolute_path, path.to_path_buf()),
    }
}

/// One Order record a batch rewrites.
#[derive(Clone, Debug, PartialEq)]
struct Rewrite {
    path: PathBuf,
    directory: PathBuf,
    prior: OrderDocument,
    candidate: OrderDocument,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchPlan {
    root: PathBuf,
    vendors_path: PathBuf,
    prior: VendorsDocument,
    candidate: VendorsDocument,
    orders: Vec<Rewrite>,
    changes: usize,
    index: Location,
    pub(crate) journal: Journal,
}

impl BatchPlan {
    /// How many Order records committing this plan rewrites.
    pub fn order_count(&self) -> usize {
        self.orders.len()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchResult {
    /// How many changes the batch held.
    pub changes: usize,
    /// How many Order records it rewrote.
    pub orders: usize,
}

/// Every record `batch` rewrites, as one plan.
pub(crate) fn plan_batch(batch: &Batch) -> Answer<BatchPlan> {
    Answer::of(|| {
        let initial = &batch.initial;
        let current = &batch.current;
        let orders = current
            .healthy_orders
            .iter()
            .zip(&initial.healthy_orders)
            .filter(|(now, then)| now.document != then.document)
            .map(|(now, then)| Rewrite {
                path: now.paths.order.absolute_path.clone(),
                directory: now.paths.directory.absolute_path.clone(),
                prior: then.document.clone(),
                candidate: now.document.clone(),
            })
            .collect::<Vec<_>>();
        let document = |package: &CurrentPackageCandidate| VendorsDocument {
            schema_version: Version1,
            vendors: package.vendors.clone(),
            values: package.values.clone(),
        };
        let prior = document(initial);
        let aggregate =
            Aggregate::judge(&document(current)).map_err(|_| Failure::InvalidAggregate)?;
        let substitution = Substitution {
            orders: orders
                .iter()
                .map(|order| (order.directory.as_path(), &order.candidate))
                .collect(),
            vendors: Some(&aggregate),
        };
        if Records::of(initial).broken_by(&substitution).is_some() {
            return Err(Failure::InvalidAggregate);
        }
        let candidate = aggregate.into_document();
        let root = initial.context.root.absolute_path.clone();
        let mut plan = BatchPlan {
            journal: Journal::new(&root),
            root,
            vendors_path: initial.context.vendors.absolute_path.clone(),
            prior,
            candidate,
            orders,
            changes: batch.changes,
            index: initial.context.index.clone(),
        };
        let records = plan.records();
        plan.journal = plan.batch_journal(
            records
                .into_iter()
                .map(|(record, candidate, subject)| {
                    let expect = record.expectation();
                    (record, candidate, expect, subject)
                })
                .collect(),
        );
        Ok(plan)
    })
}

impl BatchPlan {
    /// Each record the batch rewrites, as planned, with the bytes it becomes. The Vendor
    /// record comes first: a batch only adds and edits Vendors, so writing their record
    /// first means an interruption can leave an unused Vendor, never an Order referencing
    /// one that is not yet written.
    fn records(&self) -> Vec<(ManagedRecord, Vec<u8>, Subject)> {
        let vendors = (self.candidate != self.prior).then(|| {
            (
                ManagedRecord::vendors(self.vendors_path.clone(), self.prior.clone()),
                self.candidate.to_json_bytes().unwrap_or_default(),
                Subject::Vendors,
            )
        });
        vendors
            .into_iter()
            .chain(self.orders.iter().map(|order| {
                (
                    ManagedRecord::order(order.path.clone(), order.prior.clone()),
                    order.candidate.to_json_bytes().unwrap_or_default(),
                    Subject::Order(order.directory.clone()),
                )
            }))
            .collect()
    }

    /// The commit replacing every record in `records` together, each expected to still hold
    /// what its expectation names.
    fn batch_journal(&self, records: Vec<(ManagedRecord, Vec<u8>, Expect, Subject)>) -> Journal {
        let mut journal = Journal::new(&self.root);
        if records.is_empty() {
            return journal;
        }
        for (record, candidate, expect, subject) in records {
            let kind = match subject {
                Subject::Order(_) => OperationKind::ReplaceOrderRecord,
                _ => OperationKind::ReplaceVendorRecord,
            };
            let path = record.path().to_path_buf();
            journal.replace(path, candidate, expect, PlannedAct::Managed(kind), subject);
        }
        journal.refreshing(self.index.clone());
        journal
    }
}

pub(crate) fn commit_batch<F: FileSystem>(
    plan: BatchPlan,
    filesystem: &F,
) -> Answer<MutationReport<BatchResult>> {
    Answer::of(|| {
        let result = BatchResult {
            changes: plan.changes,
            orders: plan.orders.len(),
        };
        if plan.orders.is_empty() && plan.candidate == plan.prior {
            return Ok(MutationReport::Unchanged { result });
        }
        let changed = |path: &Path| Failure::PreconditionsChanged {
            location: located_in_package(&plan.root, path.to_path_buf()),
        };
        // Every record the batch rewrites must still mean what the batch was planned
        // against; the bytes read here are what each replacement then expects.
        let current = load_package_to_change(
            &plan.root,
            &Touched::orders(plan.orders.iter().map(|order| order.directory.clone()))
                .with_vendors(true),
            filesystem,
        )?;
        let vendors_now = VendorsDocument {
            schema_version: Version1,
            vendors: current.vendors.clone(),
            values: current.values.clone(),
        };
        if vendors_now != plan.prior {
            return Err(changed(&plan.vendors_path));
        }
        for order in &plan.orders {
            // A matching file outside active discovery, or reached through a link, is
            // not the Package record this batch was authorized to replace.
            if !current.healthy_orders.iter().any(|current| {
                current.paths.directory.absolute_path == order.directory
                    && current.document == order.prior
            }) {
                return Err(changed(&order.path));
            }
            // Every candidate is a whole Order record before anything is written.
            if order.candidate.validate().is_err() {
                return Err(Failure::commit(
                    CommitFailure::new(located_in_package(&plan.root, order.path.clone())),
                    Vec::new(),
                ));
            }
        }
        // Every record the batch rewrites must still mean what the batch was planned
        // against; the bytes read here are what each replacement then expects.
        let mut records = Vec::new();
        for (record, candidate, subject) in plan.records() {
            let observed = record.confirm(&plan.root, filesystem)?;
            records.push((record, candidate, Expect::Bytes(observed), subject));
        }
        let committed = plan.batch_journal(records).run(filesystem)?;
        let refresh = committed.refresh(&plan.root, filesystem);
        Ok(MutationReport::committed(
            result,
            committed.warnings(refresh),
        ))
    })
}
