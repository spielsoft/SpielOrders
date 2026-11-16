//! Restoring one removed Order from the Package's Removed Orders Directory to its Orders
//! Directory, under the name it was removed with. It reverses a removal: one rename moves
//! the whole directory, and nothing enumerates, reads, or rewrites its Supporting Material.

use crate::{
    Answer, Failure,
    canonical::{CurrentPackageCandidate, Touched, load_package_to_change},
    evidence::EvidenceFingerprint,
    filesystem::{DestinationExpectation, replace_validated_file_expecting, staging_path},
    metadata::OrderDocument,
    mutation::{
        CleanupFailure, CommitFailure, IndexRefresh, MutationReport, Never, OperationKind,
        PlannedOperation,
    },
    operations::{FailedOperation, FailedPath},
    ports::{FileSystem, FileSystemEntryKind, RenameOutcome},
    readme::{Location, located_in_package, package_relative_path_text},
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
    pub(crate) operations: Vec<PlannedOperation>,
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

        let mut operations = Vec::new();
        if locator.is_some() {
            operations.push(PlannedOperation::new(
                OperationKind::ReplaceOrderRecord,
                located(&root, order_record(&source.absolute_path)),
            ));
        }
        operations.push(PlannedOperation::new(
            OperationKind::MoveOrderDirectory,
            destination.clone(),
        ));
        operations.push(PlannedOperation::new(
            OperationKind::RefreshPackageIndex,
            package.context.index.clone(),
        ));
        let evidence = restore_evidence(&package, &source, &document);
        Ok(OrderRestorePlan {
            root,
            source,
            destination,
            locator,
            document,
            evidence,
            operations,
        })
    })
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
        && !package
            .vendors
            .iter()
            .any(|vendor| vendor.vendor_id == *vendor_id)
    {
        return Err(Failure::UnresolvedVendor {
            vendor_id: vendor_id.to_string(),
        });
    }
    if let Some(existing) = identity_in_use(package, &document) {
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
        || filesystem.symlink_metadata(&order_record(directory)).ok()?
            != Some(FileSystemEntryKind::File)
    {
        return None;
    }
    let bytes = filesystem.read(&order_record(directory)).ok()?;
    OrderDocument::from_json_bytes(&bytes)
        .ok()?
        .validate()
        .ok()
        .map(|validated| validated.order().clone())
}

fn identity_in_use<'a>(
    package: &'a CurrentPackageCandidate,
    document: &OrderDocument,
) -> Option<&'a Location> {
    package
        .healthy_orders
        .iter()
        .map(|order| (&order.document.order_id, &order.paths.directory))
        .chain(package.damaged_orders.iter().filter_map(|order| {
            order
                .order_id
                .as_ref()
                .map(|id| (id, &order.paths.directory))
        }))
        .find_map(|(id, directory)| (id == &document.order_id).then_some(directory))
}

fn restore_destination(package: &CurrentPackageCandidate, source: &Location) -> Location {
    let orders = package
        .context
        .orders_directory
        .as_ref()
        .expect("a resolved Package names its Orders Directory");
    located(
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
            identity_in_use(package, document).is_some().to_string(),
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
        // The locator is rewritten while the directory is still outside active scope, so
        // a failure here leaves the Package exactly as it was.
        let rewritten = if let Some(locator) = &plan.locator {
            Some(rewrite_locator(
                &plan.root,
                &plan.source.absolute_path,
                &plan.document,
                locator,
                filesystem,
            )?)
        } else {
            None
        };
        match filesystem.rename_new(&plan.source.absolute_path, &plan.destination.absolute_path) {
            Ok(RenameOutcome::Renamed) => {}
            Ok(RenameOutcome::DestinationExists) => {
                let cleanup = rewritten.map_or_else(Vec::new, |(prior, candidate)| {
                    rollback_locator(&plan, &prior, &candidate, filesystem)
                });
                return Err(Failure::commit(
                    CommitFailure::new(plan.destination.clone()),
                    cleanup,
                ));
            }
            Err(error) => {
                let cleanup = rewritten.map_or_else(Vec::new, |(prior, candidate)| {
                    rollback_locator(&plan, &prior, &candidate, filesystem)
                });
                return Err(
                    Failure::commit(CommitFailure::new(plan.destination.clone()), cleanup)
                        .unless_unsupported(
                            &plan.root,
                            &plan.source.absolute_path,
                            &error,
                            filesystem,
                        ),
                );
            }
        }
        let touched = Touched::orders([plan.destination.absolute_path.clone()]);
        let refresh = IndexRefresh::after_commit(&plan.root, &touched, filesystem);
        Ok(refresh.report(OrderRestoreResult {
            directory: plan.destination.clone(),
        }))
    })
}

fn rewrite_locator<F: FileSystem>(
    root: &Path,
    directory: &Path,
    document: &OrderDocument,
    locator: &str,
    filesystem: &F,
) -> Result<(Vec<u8>, Vec<u8>), Failure> {
    let record = order_record(directory);
    let prior = filesystem.read(&record).map_err(|error| {
        Failure::from(FailedOperation::filesystem(
            located(root, record.clone()),
            &error,
            filesystem,
        ))
    })?;
    let candidate = relocated_record(document, locator).ok_or_else(|| {
        Failure::commit(
            CommitFailure::new(located(root, record.clone())),
            Vec::new(),
        )
    })?;
    let still_holds_the_planned_record = |current: &[u8]| {
        OrderDocument::from_json_bytes(current)
            .ok()
            .and_then(|parsed| parsed.validate().ok())
            .is_some_and(|projected| projected.order() == document)
    };
    replace_validated_file_expecting(
        &record,
        &candidate,
        DestinationExpectation::Equivalent(&still_holds_the_planned_record),
        filesystem,
        |_| Ok::<(), Never>(()),
    )
    .map_err(|error| Failure::of_replacement(root, &record, error))?;
    Ok((prior, candidate))
}

fn rollback_locator<F: FileSystem>(
    plan: &OrderRestorePlan,
    prior: &[u8],
    candidate: &[u8],
    filesystem: &F,
) -> Vec<CleanupFailure> {
    let record = order_record(&plan.source.absolute_path);
    let retained = || CleanupFailure::new(located(&plan.root, record.clone()));
    // A failed move does not authorize following a replacement link or overwriting a
    // later writer. The rollback is guarded by the exact bytes this restore wrote.
    if read_record(&plan.source.absolute_path, filesystem).is_none() {
        return vec![retained()];
    }
    match replace_validated_file_expecting(
        &record,
        prior,
        DestinationExpectation::Exactly(candidate),
        filesystem,
        |_| Ok::<(), Never>(()),
    ) {
        Ok(()) => Vec::new(),
        Err(error) => {
            let mut cleanup = vec![retained()];
            if error.cleanup_failure.is_some() {
                cleanup.push(CleanupFailure::new(located(
                    &plan.root,
                    staging_path(&record).expect("a managed record has a staging path"),
                )));
            }
            cleanup
        }
    }
}

/// The record with its technical locator replaced, as this application's canonical bytes.
fn relocated_record(document: &OrderDocument, locator: &str) -> Option<Vec<u8>> {
    let mut object = serde_json::to_value(document).ok()?.as_object().cloned()?;
    object.insert(
        "package_root".into(),
        serde_json::Value::String(locator.to_owned()),
    );
    let bytes = serde_json::to_vec(&serde_json::Value::Object(object)).ok()?;
    let relocated = OrderDocument::from_json_bytes(&bytes).ok()?;
    relocated.package_root.spelled()?;
    relocated.to_json_bytes().ok()
}

fn located(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned restore path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned restore path is normalized")
}

fn order_record(directory: &Path) -> PathBuf {
    directory.join(".spielorders").join("order.json")
}
