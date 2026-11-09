//! Read-only validation of one scope — a whole Package, a Package's own artifacts, or one
//! Order — and the deterministic ephemeral repair cases its evidence authorizes.
//!
//! Every scope is narrowed from the one exhaustive Package validation the read model
//! owns, so two scopes can never disagree about the same evidence or name the same
//! condition with two different codes. Diagnosis mutates nothing.

use crate::{
    Failure,
    canonical::{DiagnosticGroup, ManagedContext, MetadataState, OrderPaths, validate_package},
    operations::{FailedOperation, FailedPath, refuse_linked_root},
    ports::{FileSystem, FileSystemEntryKind},
    readme::{Diagnostic, EvidenceScope, Location, Severity, package_relative_path_text},
    repair_cases::{RepairCase, cases_for},
};
use std::path::{Path, PathBuf};

/// The complete read-only result of validating one scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnosis {
    /// The Package this diagnosis was taken against, whose Root a repair acting on it
    /// acts on.
    pub context: ManagedContext,
    /// The Order Directories this diagnosis covers, healthy or damaged: every Order of a
    /// validated Package, the one Order of an Order scope, and none for a Package's own
    /// artifacts. A diagnosis with no findings is thereby told from one that examined
    /// nothing.
    pub examined_orders: Vec<OrderPaths>,
    /// Every finding in scope, grouped at the narrowest scope it belongs to.
    pub diagnostic_groups: Vec<DiagnosticGroup>,
    /// The repair each actionable finding offers, in the findings' order.
    pub cases: Vec<RepairCase>,
}

impl Diagnosis {
    fn new(
        context: ManagedContext,
        examined_orders: Vec<OrderPaths>,
        diagnostic_groups: Vec<DiagnosticGroup>,
        metadata_state: MetadataState,
    ) -> Self {
        let findings: Vec<Diagnostic> = diagnostic_groups
            .iter()
            .flat_map(|group| group.diagnostics.iter().cloned())
            .collect();
        // Decision: Define integrity, validation, and repair boundaries.
        let cases = match metadata_state {
            MetadataState::Unsupported => Vec::new(),
            MetadataState::Current | MetadataState::NotAssessed => cases_for(&findings),
        };
        Self {
            context,
            examined_orders,
            diagnostic_groups,
            cases,
        }
    }

    /// Whether no finding in scope carries error severity.
    pub(crate) fn is_valid(&self) -> bool {
        !self
            .findings()
            .any(|finding| finding.severity == Severity::Error)
    }

    /// Every finding in scope, in its groups' order.
    pub(crate) fn findings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostic_groups
            .iter()
            .flat_map(|group| group.diagnostics.iter())
    }

    pub(crate) fn case(&self, handle: &str) -> Option<&RepairCase> {
        self.cases.iter().find(|case| case.handle == handle)
    }
}

/// Which part of a Package a Package-rooted diagnosis covers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Breadth {
    /// Every Order Directory and cross-record constraint: what validation reports.
    Whole,
    /// The Package's own artifacts: what a Package repair acts on.
    OwnArtifacts,
}

/// The diagnosis of the Package at `selected_root`. A damaged Package is a complete
/// diagnosis whose findings are the damage, so only an unmanaged Root or a failed
/// operation refuses.
pub(crate) fn of_package<F: FileSystem>(
    selected_root: &Path,
    breadth: Breadth,
    filesystem: &F,
) -> Result<Diagnosis, Failure> {
    let report = validate_package(selected_root, filesystem)?;
    let metadata_state = report.metadata_state();
    let (examined_orders, groups) = match breadth {
        Breadth::Whole => (report.examined_orders, report.diagnostic_groups),
        Breadth::OwnArtifacts => (
            Vec::new(),
            report
                .diagnostic_groups
                .into_iter()
                .filter(|group| group.scope == EvidenceScope::Package)
                .collect(),
        ),
    };
    Ok(Diagnosis::new(
        report.context,
        examined_orders,
        groups,
        metadata_state,
    ))
}

/// The diagnosis of exactly one Order Directory, healthy or damaged.
pub(crate) fn of_order<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<Diagnosis, Failure> {
    refuse_linked_root(order_directory, filesystem)?;
    // Nothing standing at the selected path is a different answer from a filesystem
    // that would not say, and resolving the path cannot tell them apart: every failure
    // to canonicalize looks the same. Observing the kind first is what keeps a path
    // that does not exist from being reported as a Package this build could not read.
    match filesystem.symlink_metadata(order_directory) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        Ok(found) => {
            return Err(Failure::NotAnOrderDirectory {
                directory: FailedPath::Unlocated(order_directory.to_path_buf()),
                found,
            });
        }
        Err(error) => {
            return Err(filesystem_failure(order_directory, error, filesystem));
        }
    }
    let directory = filesystem
        .canonicalize(order_directory)
        .map_err(|error| filesystem_failure(order_directory, error, filesystem))?;
    // Duplicate Order IDs and Vendor References are cross-record constraints, so one
    // Order cannot be judged from its own directory alone. The Package-wide pass
    // supplies that evidence; only its own scope is reported.
    let root = package_root_of(&directory, filesystem)?;
    let report = validate_package(&root, filesystem)?;

    // A Package-wide failure is never downgraded to an Order finding. Answering
    // "this Order looks fine" while the Package it belongs to is untrustworthy would
    // be worse than refusing, because the Order-scoped answer depends on
    // package-wide evidence such as the Vendor aggregate.
    if let Some(unusable) = report.package_damage() {
        return Err(unusable);
    }

    // Presence in the Package's own discovery is what proves the scope exists: a healthy
    // Order produces no diagnostic group at all, and a directory that is not an entry of
    // the configured Orders Directory has no Order scope to report whatever stands under
    // it. The paths come from the same place for the same reason — they are the Package's
    // own account of where this Order's documents are.
    let Some(paths) = report
        .examined_orders
        .iter()
        .find(|paths| paths.directory.absolute_path == directory)
        .cloned()
    else {
        return Err(Failure::OrderNotFound {
            directory: located_within(&root, &directory),
            root: Some(root),
        });
    };
    let metadata_state = report.metadata_state();
    let groups = report
        .diagnostic_groups
        .into_iter()
        .filter(|group| {
            group.scope != EvidenceScope::Package
                && group.location.absolute_path.starts_with(&directory)
        })
        .collect();
    Ok(Diagnosis::new(
        report.context,
        vec![paths],
        groups,
        metadata_state,
    ))
}

/// One path this Package can name, in the best spelling it has for it.
fn located_within(root: &Path, path: &Path) -> FailedPath {
    match package_relative_path_text(root, path)
        .and_then(|relative| Location::within_package(path.to_path_buf(), relative).ok())
    {
        Some(location) => FailedPath::Located(location),
        None => FailedPath::of(path),
    }
}

/// Resolves the Package that owns this directory from its position beneath a root
/// carrying a Package marker, so a Damaged Order with no readable record can still
/// be diagnosed.
pub(crate) fn package_root_of<F: FileSystem>(
    directory: &Path,
    filesystem: &F,
) -> Result<PathBuf, Failure> {
    for candidate in directory.ancestors().skip(1) {
        let marker = candidate.join(".spielorders").join("version.json");
        match filesystem.symlink_metadata(&marker) {
            Ok(Some(_)) => return Ok(candidate.to_path_buf()),
            Ok(None) => {}
            // The marker is the Package's own metadata, so a failure to observe it is a
            // Package that could not be read.
            Err(error) => {
                return Err(Failure::unobservable(&marker, &error, filesystem));
            }
        }
    }
    Err(Failure::unmanaged(directory))
}

/// A failure to observe or resolve the selected directory.
fn filesystem_failure<F: FileSystem>(path: &Path, error: F::Error, filesystem: &F) -> Failure {
    FailedOperation::filesystem(FailedPath::of(path), &error, filesystem).into()
}
