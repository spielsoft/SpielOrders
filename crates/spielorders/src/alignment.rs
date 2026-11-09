//! Planning and persistence for one explicit Order Directory alignment.
//!
//! Alignment is the only operation that renames an Order Directory. Fact edits,
//! status changes, and Vendor changes deliberately leave directory drift in place,
//! so a caller must ask for this explicitly.

use crate::{
    Answer, Failure,
    canonical::{CurrentPackageCandidate, HealthyOrder},
    creation::{normalize_filename_component, projected_order_basename},
    evidence::EvidenceFingerprint,
    metadata::{OrderDocument, VendorId},
    mutation::{CommitFailure, IndexRefresh, MutationReport, OperationKind, PlannedOperation},
    operations::FailedOperation,
    order_edits::resolve_order,
    ports::{FileSystem, FileSystemEntryKind, NewDirectoryRenameOutcome},
    readme::{Location, package_relative_path_text},
};
use std::path::{Path, PathBuf};

/// One complete, validated alignment of one healthy Order Directory.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderAlignmentPlan {
    root: PathBuf,
    source: Location,
    destination: Location,
    document: OrderDocument,
    evidence: EvidenceFingerprint,
    pub(crate) operations: Vec<PlannedOperation>,
}

impl OrderAlignmentPlan {
    /// Whether committing this plan would rename the directory. Drift is ordinary
    /// allowed state, so an already aligned Order plans no operation at all.
    pub fn changes_directory_name(&self) -> bool {
        self.source.absolute_path != self.destination.absolute_path
    }
}

pub(crate) fn plan_order_alignment<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Answer<OrderAlignmentPlan> {
    Answer::of(|| {
        let (package, order) = resolve_order(order_directory, filesystem)?;
        let document = order.document.clone();
        let basename = projected_basename(&document, &package)?;

        let root = package.context.root.absolute_path.clone();
        let source = order.paths.directory.clone();
        let parent = source
            .absolute_path
            .parent()
            .ok_or_else(|| Failure::OrderNotFound {
                directory: source.clone().into(),
                root: Some(root.clone()),
            })?
            .to_path_buf();
        let destination = located(&root, parent.join(&basename));
        let index = package.context.index.clone();

        let operations = if source.absolute_path == destination.absolute_path {
            Vec::new()
        } else {
            // The projected name must be free. A collision stops alignment; it never
            // invents a suffix and never replaces whatever is already there.
            match filesystem.symlink_metadata(&destination.absolute_path) {
                Ok(None) => {}
                Ok(found @ Some(_)) => {
                    return Err(Failure::DestinationCollision {
                        root: package.context.root.clone(),
                        location: destination.clone(),
                        found,
                    });
                }
                Err(error) => {
                    return Err(Failure::from(
                        FailedOperation::filesystem(destination.clone(), &error, filesystem)
                            .under(&package.context.root),
                    ));
                }
            }
            vec![
                PlannedOperation::new(OperationKind::RenameOrderDirectory, destination.clone()),
                PlannedOperation::new(OperationKind::RefreshPackageIndex, index),
            ]
        };

        let evidence = alignment_evidence(&package, &order);
        Ok(OrderAlignmentPlan {
            root,
            source,
            destination,
            document,
            // An alignment moves the README with its directory, so what the load knew
            // before the rename is what the renamed Order holds.
            evidence,
            operations,
        })
    })
}

/// Projects the aligned basename from the Order's current facts and its live Vendor
/// Name. An Order retains no historical Vendor snapshot, so the name follows the
/// Vendor record as it stands now.
fn projected_basename(
    document: &OrderDocument,
    package: &CurrentPackageCandidate,
) -> Result<String, Failure> {
    let vendor_name = match document.vendor_id.as_ref() {
        Some(vendor_id) => Some(resolved_vendor_name(vendor_id, package).ok_or_else(|| {
            Failure::UnresolvedVendor {
                vendor_id: vendor_id.as_str().to_owned(),
            }
        })?),
        None => None,
    };
    let title_component = normalize_filename_component(document.title.as_str());
    if title_component.is_empty() {
        return Err(Failure::EmptyProjectedComponent);
    }
    Ok(projected_order_basename(
        document.date_ordered.as_str(),
        vendor_name,
        &title_component,
        document.orderer.as_deref(),
    ))
}

/// The current Name of one referenced Vendor, resolved live from the Package. `None` is
/// an Unresolved Vendor Reference, which is an integrity failure and not an Order
/// without a Vendor; callers distinguish the two by whether a reference exists at all.
fn resolved_vendor_name<'a>(
    vendor_id: &VendorId,
    package: &'a CurrentPackageCandidate,
) -> Option<&'a str> {
    package
        .vendors
        .iter()
        .find(|vendor| vendor.vendor_id == *vendor_id)
        .map(|vendor| vendor.name.as_str())
}

/// The path an alignment of `source` would rename it to, projected from the Order's
/// facts as they stand in `package`. `None` when those facts project no aligned name
/// at all, which is a refusal and never a rename to some fallback.
fn projected_destination(
    source: &Path,
    document: &OrderDocument,
    package: &CurrentPackageCandidate,
) -> Option<PathBuf> {
    // The same projection the plan used, so plan-time and commit-time answers cannot
    // drift apart through two spellings of one rule.
    let basename = projected_basename(document, package).ok()?;
    Some(source.parent()?.join(basename))
}

/// A path as its exact bytes. A path is not required to be UTF-8, and a comparison
/// that silently repaired one would lose exactly the difference this seam looks for.
fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

/// The facts an alignment plan is authorized against, as `order` and `package` stand
/// now.
///
/// Alignment's one effect is a rename, decided by *which* Order is being renamed and
/// *what* name its current facts project. Neither is visible in the path, so neither
/// is covered by confirming that the path still exists: `symlink_metadata` reports an
/// ordinary directory whether the Order standing there is the planned one or a
/// different one left in its place, and an ordinary fact edit or Vendor rename
/// deliberately leaves the directory alone while making the plan's projected name the
/// wrong one. Each fact is recorded here and re-observed through
/// [`EvidenceFingerprint::confirm`] immediately before that rename.
fn alignment_evidence(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
) -> EvidenceFingerprint {
    let document = &order.document;
    // Read from the resolved Order rather than taken from the plan, because comparing
    // the plan's own copy of its source against itself would confirm nothing. Identity
    // is what separates one Order from another here; this fact carries the location the
    // destination below is projected from.
    let source = &order.paths.directory.absolute_path;
    let fingerprint = EvidenceFingerprint::new()
        // Identity, because the path is location and not identity: an Order ID is what
        // separates this Order from any other that could stand at the same path.
        .observed("order_id", document.order_id.as_str())
        .observed("source", path_bytes(source))
        // The Package the source belongs to, resolved through the Order's own locator.
        // An Order that has come to belong somewhere else is not this plan's subject
        // even where its identity and its path both still match.
        .observed(
            "package_root",
            path_bytes(&package.context.root.absolute_path),
        )
        // Every fact the basename projects from, each under its own name, so a refusal
        // says which one moved rather than only that the projected name would differ.
        .observed("title", document.title.as_str())
        .observed("date_ordered", document.date_ordered.as_str());
    let fingerprint = match document.orderer.as_deref() {
        Some(orderer) => fingerprint.observed("orderer", orderer),
        None => fingerprint.absent("orderer"),
    };
    let fingerprint = match document.vendor_id.as_ref() {
        Some(vendor_id) => fingerprint.observed("vendor_id", vendor_id.as_str()),
        None => fingerprint.absent("vendor_id"),
    };
    // The live Vendor Name, not the reference alone. An Order keeps no Vendor snapshot,
    // so renaming the Vendor changes the name this Order projects while every Order
    // fact, the Vendor ID, and the source path all still stand.
    // An unresolved reference records a present `vendor_id` with no name, which no
    // absent reference can be mistaken for.
    let fingerprint = match document
        .vendor_id
        .as_ref()
        .and_then(|vendor_id| resolved_vendor_name(vendor_id, package))
    {
        Some(name) => fingerprint.observed("vendor_name", name),
        None => fingerprint.absent("vendor_name"),
    };
    // The name the rename would actually produce. The facts above are what a refusal
    // names; this is the projection itself, so a change in how those facts compose
    // into a basename cannot pass while each fact individually matches.
    match projected_destination(source, document, package) {
        Some(destination) => fingerprint.observed("destination", path_bytes(&destination)),
        None => fingerprint.absent("destination"),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderAlignmentResult {
    /// The Order's Directory, at its new path.
    pub directory: Location,
}

pub(crate) type OrderAlignmentReport = MutationReport<OrderAlignmentResult>;

pub(crate) fn commit_order_alignment<F: FileSystem + FileSystem>(
    plan: OrderAlignmentPlan,
    filesystem: &F,
) -> Answer<OrderAlignmentReport> {
    Answer::of(|| {
        match filesystem.symlink_metadata(&plan.source.absolute_path) {
            Ok(Some(FileSystemEntryKind::Directory)) => {}
            // A source that vanished or is no longer an ordinary directory is not the
            // Order this plan validated. Checked here rather than left to the resolution
            // below, which canonicalizes: a link standing where the directory was would
            // resolve to whatever it points at and then be renamed in its place.
            _ => return Err(preconditions_changed(plan.source.clone())),
        }

        // Re-resolve the subject rather than settle for having found something there. A
        // source that no longer resolves to a healthy Order of its own Package is neither
        // this plan's Order nor identifiable as any other.
        let (package, order) = match resolve_order(&plan.source.absolute_path, filesystem) {
            Ok(resolved) => resolved,
            Err(
                Failure::OrderNotFound { .. }
                | Failure::DamagedRecord { .. }
                | Failure::DamagedOrder { .. },
            ) => return Err(preconditions_changed(plan.source.clone())),
            Err(failure) => return Err(failure),
        };

        // Immediately before the first effect, which is the rename below or — for an
        // already aligned Order — the report's standing claim that this Order's facts
        // project the name it already carries.
        let outcome = plan.evidence.confirm(&alignment_evidence(&package, &order));
        if !outcome.is_unchanged() {
            return Err(Failure::PlanEvidenceUnconfirmed { outcome });
        }

        // An already aligned Order is committed by performing nothing.
        if !plan.changes_directory_name() {
            let result = OrderAlignmentResult {
                directory: plan.source.clone(),
            };
            return Ok(MutationReport::Unchanged { result });
        }

        // The whole directory moves as one opaque tree. Nothing enumerates, copies, or
        // rewrites its children, so README and Supporting Material bytes travel intact.
        match filesystem
            .rename_new_directory(&plan.source.absolute_path, &plan.destination.absolute_path)
        {
            Ok(NewDirectoryRenameOutcome::Renamed) => {}
            Ok(NewDirectoryRenameOutcome::DestinationExists) => {
                return Err(Failure::DestinationCollision {
                    root: located(&plan.root, plan.root.clone()),
                    location: plan.destination.clone(),
                    found: None,
                });
            }
            Err(_) => {
                let failure = CommitFailure::new(plan.destination.clone());
                return Err(Failure::commit(failure, Vec::new()));
            }
        }

        let mut refresh = IndexRefresh::after_commit(&plan.root, filesystem);
        // The rename already happened, so an Order the refresh cannot show is never a
        // rejection: the caller learns it as a warning rather than as work that did not
        // occur.
        if refresh
            .package()
            .and_then(|package| order_at(package, &plan))
            .is_none()
        {
            refresh.unconfirmed();
        }
        Ok(refresh.report(OrderAlignmentResult {
            directory: plan.destination.clone(),
        }))
    })
}

fn order_at(package: &CurrentPackageCandidate, plan: &OrderAlignmentPlan) -> Option<HealthyOrder> {
    package
        .healthy_orders
        .iter()
        .find(|candidate| candidate.document.order_id == plan.document.order_id)
        .cloned()
}

/// One planned path, as this Package locates it.
///
/// The Package-relative spelling holds by construction, as it does for a planned creation
/// artifact: the source is a discovered Order Directory, whose own `Location` the load
/// already carries, and the destination is that directory's parent joined with a
/// projected basename `normalize_filename_component` has already stripped of separators.
fn located(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned alignment path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned alignment path is normalized")
}

fn preconditions_changed(location: Location) -> Failure {
    Failure::PreconditionsChanged { location }
}
