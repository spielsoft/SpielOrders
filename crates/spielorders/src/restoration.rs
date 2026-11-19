//! Restoring one removed Order from the Package's Removed Orders Directory to its Orders
//! Directory, under the name it was removed with. It reverses a removal: one rename moves
//! the whole directory, and nothing enumerates, reads, or rewrites its Supporting Material.

use crate::{
    Answer, Failure,
    canonical::{CurrentPackageCandidate, Touched, load_package_to_change},
    change::PlannedAct,
    evidence::EvidenceFingerprint,
    integrity::{Records, resolves},
    journal::{Journal, ManagedRecord, Subject, planned_location},
    metadata::OrderDocument,
    mutation::{CommitFailure, MutationReport, OperationKind},
    operations::{FailedOperation, FailedPath},
    ports::{FileSystem, FileSystemEntryKind},
    readme::{Location, located_in_package, package_relative_path_text},
    removal::relocated_record,
};
use std::path::{Path, PathBuf};

/// One validated restore of one removed Order Directory.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderRestorePlan {
    root: PathBuf,
    source: Location,
    destination: Location,
    /// The locator the record needs in the Orders Directory, where it differs.
    locator: Option<String>,
    document: OrderDocument,
    evidence: EvidenceFingerprint,
    pub(crate) journal: Journal,
}

impl OrderRestorePlan {
    pub(crate) fn order_id(&self) -> &str {
        self.document.order_id.as_str()
    }
}

/// Where the restored Order went.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderRestoreResult {
    pub directory: Location,
}

pub(crate) type OrderRestoreReport = MutationReport<OrderRestoreResult>;

pub(crate) fn plan_order_restore<F: FileSystem>(
    root: &Path,
    removed: &Path,
    filesystem: &F,
) -> Answer<OrderRestorePlan> {
    Answer::of(|| {
        let package =
            load_package_to_change(root, &Touched::default().with_vendors(true), filesystem)?;
        let (source, document) = removed_order(&package, removed, filesystem)?;
        let root = package.context.root.absolute_path.clone();
        let destination = restore_destination(&package, &source);
        match filesystem.symlink_metadata(&destination.absolute_path) {
            Ok(None) => {}
            Ok(found) => {
                return Err(Failure::DestinationCollision {
                    root: package.context.root.clone(),
                    location: destination,
                    found,
                });
            }
            Err(error) => {
                return Err(Failure::from(
                    FailedOperation::filesystem(destination, &error, filesystem)
                        .under(&package.context.root),
                ));
            }
        }
        let depth = package
            .context
            .orders_directory
            .as_ref()
            .expect("a resolved Package names its Orders Directory")
            .package_relative_path
            .as_deref()
            .map_or(0, |relative| relative.split('/').count())
            + 1;
        let projected = vec![".."; depth].join("/");
        let locator =
            (document.package_root.stored_text() != Some(projected.as_str())).then_some(projected);

        let journal = restore_journal(
            &root,
            &source,
            &destination,
            relocation(&source, &document, locator.as_deref()),
            package.context.index.clone(),
        );
        let evidence = restore_evidence(&package, &source, &document);
        Ok(OrderRestorePlan {
            root,
            source,
            destination,
            locator,
            document,
            evidence,
            journal,
        })
    })
}

/// The record at `source` with the locator it needs in the Orders Directory, where that
/// differs from the one it stores: the record planned against, and the bytes it becomes.
/// The candidate is empty where the plan's own record no longer relocates, which is an
/// internal bug rather than a state of the Package, and a commit refuses it.
fn relocation(
    source: &Location,
    document: &OrderDocument,
    locator: Option<&str>,
) -> Option<(ManagedRecord, Vec<u8>)> {
    let candidate = relocated_record(document, locator?).unwrap_or_default();
    let record = ManagedRecord::order(record_path(&source.absolute_path), document.clone());
    Some((record, candidate))
}

/// The commit restoring the removed Order at `source` to `destination`: its record
/// rewritten with the locator it needs there while the directory is still outside active
/// scope, so a failure leaves the Package as it was, and then the whole directory moved by
/// one rename.
fn restore_journal(
    root: &Path,
    source: &Location,
    destination: &Location,
    relocation: Option<(ManagedRecord, Vec<u8>)>,
    index: Location,
) -> Journal {
    let managed = PlannedAct::Managed;
    let mut journal = Journal::new(root);
    if let Some((record, candidate)) = relocation {
        journal.replace(
            record.path().to_path_buf(),
            candidate,
            record.expectation(),
            managed(OperationKind::ReplaceOrderRecord),
            Subject::Other,
        );
    }
    journal.move_entry(
        source.absolute_path.clone(),
        destination.absolute_path.clone(),
        managed(OperationKind::MoveOrderDirectory),
        Subject::Order(destination.absolute_path.clone()),
    );
    journal.refreshing(index);
    journal
}

/// The removed Order at `removed`: a direct child of this Package's Removed Orders
/// Directory, an ordinary directory rather than a link, holding a readable Order record
/// whose Order ID no Order of the Package carries.
fn removed_order<F: FileSystem>(
    package: &CurrentPackageCandidate,
    removed: &Path,
    filesystem: &F,
) -> Result<(Location, OrderDocument), Failure> {
    let root = &package.context.root;
    let failed = |path: &Path, error: &F::Error| -> Failure {
        FailedOperation::filesystem(FailedPath::Unlocated(path.to_path_buf()), error, filesystem)
            .under(root)
            .into()
    };
    let entry: PathBuf = removed.components().collect();
    match filesystem.symlink_metadata(&entry) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        Ok(found) => {
            return Err(Failure::NotAnOrderDirectory {
                directory: FailedPath::Unlocated(entry),
                found,
            });
        }
        Err(error) => return Err(failed(&entry, &error)),
    }
    let directory = filesystem
        .canonicalize(&entry)
        .map_err(|error| failed(&entry, &error))?;
    let removed_orders = package
        .context
        .removed_orders_directory
        .as_ref()
        .expect("a resolved Package names its Removed Orders Directory");
    let parent = directory.parent().map(Path::to_path_buf);
    let source = match package_relative_path_text(&root.absolute_path, &directory) {
        Some(relative) if parent.as_deref() == Some(removed_orders.absolute_path.as_path()) => {
            Location::within_package(directory.clone(), relative)
                .expect("a removed Order Directory is normalized")
        }
        _ => {
            return Err(Failure::NotDirectChild {
                location: located_in_package(&root.absolute_path, directory),
            });
        }
    };
    let document =
        read_record(&source.absolute_path, filesystem).ok_or_else(|| Failure::DamagedRecord {
            directory: FailedPath::Located(source.clone()),
            field: None,
        })?;
    if let Some(vendor_id) = &document.vendor_id
        && !resolves(&package.vendors, vendor_id)
    {
        return Err(Failure::UnresolvedVendor {
            vendor_id: vendor_id.to_string(),
        });
    }
    if let Some(existing) = Records::of(package).holder(&document.order_id, None) {
        return Err(Failure::OrderIdentityInUse {
            order_id: document.order_id.to_string(),
            location: existing.clone(),
        });
    }
    Ok((source, document))
}

/// The removed Order's record as the load would project it, or `None` where it is not
/// one.
fn read_record<F: FileSystem>(directory: &Path, filesystem: &F) -> Option<OrderDocument> {
    if filesystem
        .symlink_metadata(&directory.join(".spielorders"))
        .ok()?
        != Some(FileSystemEntryKind::Directory)
        || filesystem.symlink_metadata(&record_path(directory)).ok()?
            != Some(FileSystemEntryKind::File)
    {
        return None;
    }
    let bytes = filesystem.read(&record_path(directory)).ok()?;
    OrderDocument::from_json_bytes(&bytes)
        .ok()?
        .validate()
        .ok()
        .map(|validated| validated.order().clone())
}

fn restore_destination(package: &CurrentPackageCandidate, source: &Location) -> Location {
    let orders = package
        .context
        .orders_directory
        .as_ref()
        .expect("a resolved Package names its Orders Directory");
    planned_location(
        &package.context.root.absolute_path,
        orders.absolute_path.join(
            source
                .absolute_path
                .file_name()
                .expect("a removed Order Directory has a basename"),
        ),
    )
}

/// What a restore plan is authorized against: which removed Order, where it goes, and
/// that no Order of the Package has taken its identity since.
fn restore_evidence(
    package: &CurrentPackageCandidate,
    source: &Location,
    document: &OrderDocument,
) -> EvidenceFingerprint {
    let destination = restore_destination(package, source);
    EvidenceFingerprint::new()
        .observed("order_id", document.order_id.as_str())
        .observed(
            "source",
            source.absolute_path.as_os_str().as_encoded_bytes(),
        )
        .observed(
            "package_root",
            package
                .context
                .root
                .absolute_path
                .as_os_str()
                .as_encoded_bytes(),
        )
        .observed(
            "destination",
            destination.absolute_path.as_os_str().as_encoded_bytes(),
        )
        .observed(
            "identity_in_use",
            Records::of(package)
                .holder(&document.order_id, None)
                .is_some()
                .to_string(),
        )
        .observed("order_record", format!("{document:?}"))
}

pub(crate) fn commit_order_restore<F: FileSystem>(
    plan: OrderRestorePlan,
    filesystem: &F,
) -> Answer<OrderRestoreReport> {
    Answer::of(|| {
        let changed = || Failure::PreconditionsChanged {
            location: plan.source.clone(),
        };
        let package = load_package_to_change(
            &plan.root,
            &Touched::default().with_vendors(true),
            filesystem,
        )
        .map_err(|_| changed())?;
        let (source, document) = removed_order(&package, &plan.source.absolute_path, filesystem)
            .map_err(|_| changed())?;
        let outcome = plan
            .evidence
            .confirm(&restore_evidence(&package, &source, &document));
        if !outcome.is_unchanged() {
            return Err(Failure::PlanEvidenceUnconfirmed { outcome });
        }
        match filesystem.symlink_metadata(&plan.destination.absolute_path) {
            Ok(None) => {}
            _ => return Err(changed()),
        }
        let relocation = relocation(&plan.source, &plan.document, plan.locator.as_deref());
        if relocation
            .as_ref()
            .is_some_and(|(_, candidate)| candidate.is_empty())
        {
            let record = record_path(&plan.source.absolute_path);
            return Err(Failure::commit(
                CommitFailure::new(planned_location(&plan.root, record)),
                Vec::new(),
            ));
        }
        let committed = restore_journal(
            &plan.root,
            &plan.source,
            &plan.destination,
            relocation,
            package.context.index.clone(),
        )
        .run(filesystem)?;
        let refresh = committed.refresh(&plan.root, filesystem);
        Ok(MutationReport::committed(
            OrderRestoreResult {
                directory: plan.destination.clone(),
            },
            committed.warnings(refresh),
        ))
    })
}

fn record_path(directory: &Path) -> PathBuf {
    directory.join(".spielorders").join("order.json")
}
