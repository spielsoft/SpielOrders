//! Many record changes as one guarded write: each change is planned against the Package
//! as the changes before it leave it, in order, and the whole batch commits every record
//! it changed together, or nothing.
//!
//! Decision sources: Define the Rust CLI capability contract; Durable managed-file writes.

use crate::{
    Answer, Change, Failure, OrderRef,
    canonical::{CurrentPackageCandidate, Touched, load_package_to_change},
    filesystem::{FileReplacement, replace_validated_files},
    metadata::{OrderDocument, VendorId, VendorsDocument, Version1},
    mutation::{IndexRefresh, MutationReport, OperationKind, PlannedOperation},
    order_edits::edited_order,
    package::PackageMetadata,
    ports::{FileSystem, FileSystemEntryKind, LocalDate},
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
    /// accepted, applies it there; a refused change leaves the batch as it was.
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
    ) -> Result<(), Failure> {
        let package = &self.current;
        match change {
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
                let plan = edited_order(package, order, assignments, Some(items), today)
                    .map_err(Failure::InvalidDraft)?;
                let replacement = order
                    .with_document(&plan.facts.document)
                    .ok_or(Failure::InvalidFacts)?;
                let values = crate::values::merged(&package.values, plan.additions());
                self.current.replace_order(position, replacement, values);
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
            }
            _ => return Err(Failure::NotBatchable),
        }
        self.changes += 1;
        Ok(())
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
    pub(crate) operations: Vec<PlannedOperation>,
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
        let candidate = PackageMetadata::new(
            &current
                .healthy_orders
                .iter()
                .map(|order| order.document.clone())
                .collect::<Vec<_>>(),
            &document(current),
        )
        .validate()
        .map_err(|_| Failure::InvalidAggregate)?
        .vendors()
        .clone();
        let root = &initial.context.root.absolute_path;
        let mut operations = orders
            .iter()
            .map(|order| {
                PlannedOperation::new(
                    OperationKind::ReplaceOrderRecord,
                    located_in_package(root, order.path.clone()),
                )
            })
            .collect::<Vec<_>>();
        if candidate != prior {
            operations.push(PlannedOperation::new(
                OperationKind::ReplaceVendorRecord,
                initial.context.vendors.clone(),
            ));
        }
        if !operations.is_empty() {
            operations.push(PlannedOperation::new(
                OperationKind::RefreshPackageIndex,
                initial.context.index.clone(),
            ));
        }
        Ok(BatchPlan {
            root: root.clone(),
            vendors_path: initial.context.vendors.absolute_path.clone(),
            prior,
            candidate,
            orders,
            changes: batch.changes,
            operations,
        })
    })
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
        if plan.operations.is_empty() {
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
        let mut candidates = Vec::new();
        for order in &plan.orders {
            // A matching file outside active discovery, or reached through a link, is
            // not the Package record this batch was authorized to replace.
            if !current.healthy_orders.iter().any(|current| {
                current.paths.directory.absolute_path == order.directory
                    && current.document == order.prior
            }) {
                return Err(changed(&order.path));
            }
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
        if plan.candidate != plan.prior {
            let observed = filesystem
                .read(&plan.vendors_path)
                .map_err(|_| changed(&plan.vendors_path))?;
            let holds_prior = VendorsDocument::from_json_bytes(&observed)
                .ok()
                .and_then(|document| PackageMetadata::new(&[], &document).validate().ok())
                .is_some_and(|projected| projected.vendors() == &plan.prior);
            if !holds_prior {
                return Err(changed(&plan.vendors_path));
            }
            let candidate = plan
                .candidate
                .to_json_bytes()
                .map_err(|_| Failure::InvalidAggregate)?;
            candidates.push((plan.vendors_path.clone(), candidate, observed));
        }
        let replacements = candidates
            .iter()
            .map(|(path, candidate, observed)| FileReplacement::observed(path, candidate, observed))
            .collect::<Vec<_>>();
        let vendors_path = plan.vendors_path.as_path();
        replace_validated_files(&replacements, filesystem, |destination, candidate| {
            if destination == vendors_path {
                Ok(())
            } else {
                OrderDocument::from_json_bytes(candidate)
                    .ok()
                    .and_then(|document| document.validate().ok())
                    .map(|_| ())
                    .ok_or(())
            }
        })
        .map_err(|error| {
            let destinations = replacements
                .iter()
                .map(FileReplacement::destination)
                .collect::<Vec<_>>();
            Failure::of_replacements(&plan.root, &destinations, error)
        })?;
        let touched = Touched::orders(plan.orders.iter().map(|order| order.directory.clone()))
            .with_vendors(plan.candidate != plan.prior);
        Ok(IndexRefresh::after_commit(&plan.root, &touched, filesystem).report(result))
    })
}
