//! Read-only scoped validation of exactly one Order, and the deterministic ephemeral
//! repair cases its evidence authorizes.
//!
//! Diagnosis mutates nothing. It reuses the exhaustive Package validation the read
//! model already owns and narrows the result to one Order, so a scoped diagnosis and
//! a Package-wide one can never disagree about the same evidence or name the same
//! condition with two different codes.

use crate::{
    canonical::{
        DiagnosticGroup, ExhaustivePackageValidation, OperationFailedProjection, OrderPaths,
        validate_package_exhaustively,
    },
    readme::{Diagnostic, EvidenceScope, FailedPath, Location, package_relative_path_text},
    repair_cases::{RepairCase, UnactionableFinding, cases_for},
};
use std::path::{Path, PathBuf};

/// The complete read-only result of validating one selected Order scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderDiagnosis {
    paths: OrderPaths,
    root: PathBuf,
    valid: bool,
    diagnostics: Vec<Diagnostic>,
    cases: Vec<RepairCase>,
    unactionable: Vec<UnactionableFinding>,
}

impl OrderDiagnosis {
    /// The exact Order Directory this diagnosis examined.
    pub fn directory(&self) -> &Path {
        self.paths.directory().absolute_path()
    }

    /// This Order's managed locations, as the Package that holds it names them.
    ///
    /// **The Package-relative spelling comes from here rather than from a caller.** A
    /// response that reports this Order's Directory owes that spelling, and a caller
    /// deriving one from the absolute path would be a second place deciding what a
    /// Package's layout is.
    pub fn paths(&self) -> &OrderPaths {
        &self.paths
    }

    /// The SpielOrders Root of the Package this diagnosis was taken against.
    ///
    /// Reported rather than left to be re-derived, because every conclusion here
    /// depends on it: the Orders Directory is configured and may sit at any depth, so
    /// counting parents from the Order Directory names an ordinary folder of the
    /// user's. A repair acting on this diagnosis acts on this Package.
    pub fn package_root(&self) -> &Path {
        &self.root
    }

    /// Whether this Order scope carries no error-severity diagnostic.
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Every diagnostic in this Order's scope, in the established deterministic
    /// order, using the codes and evidence the Package-wide validation reports.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn cases(&self) -> &[RepairCase] {
        &self.cases
    }

    pub fn case(&self, handle: &str) -> Option<&RepairCase> {
        self.cases.iter().find(|case| case.handle() == handle)
    }

    /// Diagnosed conditions this core offers no safe action for. They are reported
    /// rather than silently omitted, so a caller can tell "nothing is wrong" from
    /// "nothing safe can be done from here".
    pub fn unactionable(&self) -> &[UnactionableFinding] {
        &self.unactionable
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderDiagnosisError<E> {
    /// No SpielOrders Root holds the selected directory: no ancestor of it carries a
    /// Package marker, or what did stopped being a Root between the search and the load.
    /// Either way there is no Package for an Order-scoped diagnosis to be taken against,
    /// and the path reported is the directory the search started from.
    UnmanagedPackage {
        directory: PathBuf,
    },
    /// Package-wide state is untrustworthy, so an Order-scoped answer would be
    /// misleading. A Package-wide failure is never downgraded to an Order finding.
    ///
    /// **The Package-scope findings travel with it**, and only those: a caller has to be
    /// able to report the Package-level problem *as* a Package-level problem, and a
    /// sibling Order's damage is not this scope's to report. Before they did, the refusal
    /// named the state and discarded the evidence for it, so the only thing a caller
    /// could say was that something somewhere was wrong.
    DamagedPackage {
        root: Location,
        diagnostics: Vec<Diagnostic>,
    },
    /// The Package could not be read past its Root.
    PackageOperationFailed(Box<OperationFailedProjection>),
    /// The exact path given is not a directory this diagnosis can examine: nothing
    /// stands there, or what does is not an ordinary directory.
    ///
    /// **Told apart from a filesystem that refused to say**, exactly as the ordinary
    /// Order read tells them apart: the first is "this is not an Order Directory" and
    /// the second is an operational failure. A caller that folded them would report a
    /// path that does not exist as a Package it could not read.
    NotAnOrderDirectory {
        directory: PathBuf,
    },
    /// The selected directory is not an entry of this Package's Orders Directory.
    ///
    /// The directory carries its Package-relative spelling where it has one, because a
    /// directory that is inside the Package and is not one of its Orders is exactly the
    /// case a reader needs named both ways.
    ///
    /// **The Root travels with it**, because that spelling is relative to something and a
    /// caller handed only the relative half has nothing to resolve it against. This
    /// refusal is raised after the Package resolved and validated — the directory is
    /// judged against the Orders that load discovered — so a Root is exactly what it has.
    OrderNotFound {
        directory: FailedPath,
        root: PathBuf,
    },
    FileSystem {
        path: PathBuf,
        error: E,
    },
}

/// Validates exactly one Order Directory, healthy or damaged, and diagnoses the
/// explicit safe repairs its evidence authorizes.
pub fn diagnose_order<F: crate::ports::FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<OrderDiagnosis, OrderDiagnosisError<F::Error>> {
    // Nothing standing at the selected path is a different answer from a filesystem
    // that would not say, and resolving the path cannot tell them apart: every failure
    // to canonicalize looks the same. Observing the kind first is what keeps a path
    // that does not exist from being reported as a Package this build could not read.
    match filesystem.symlink_metadata(order_directory) {
        Ok(Some(crate::ports::FileSystemEntryKind::Directory)) => {}
        Ok(_) => {
            return Err(OrderDiagnosisError::NotAnOrderDirectory {
                directory: order_directory.to_path_buf(),
            });
        }
        Err(error) => {
            return Err(OrderDiagnosisError::FileSystem {
                path: order_directory.to_path_buf(),
                error,
            });
        }
    }
    let directory = filesystem.canonicalize(order_directory).map_err(|error| {
        OrderDiagnosisError::FileSystem {
            path: order_directory.to_path_buf(),
            error,
        }
    })?;
    // Duplicate Order IDs and Vendor References are cross-record constraints, so one
    // Order cannot be judged from its own directory alone. The Package-wide pass
    // supplies that evidence; only its own scope is reported.
    let root = package_root_of(&directory, filesystem)?;
    let report = match validate_package_exhaustively(&root, filesystem) {
        ExhaustivePackageValidation::Complete(report) => report,
        ExhaustivePackageValidation::Unmanaged(_) => {
            return Err(OrderDiagnosisError::UnmanagedPackage { directory });
        }
        ExhaustivePackageValidation::OperationFailed(failed) => {
            return Err(OrderDiagnosisError::PackageOperationFailed(Box::new(
                failed,
            )));
        }
    };

    // A Package-wide failure is never downgraded to an Order finding. Answering
    // "this Order looks fine" while the Package it belongs to is untrustworthy would
    // be worse than refusing, because the Order-scoped answer depends on
    // package-wide evidence such as the Vendor aggregate.
    let package_wide = report
        .diagnostic_groups()
        .iter()
        .filter(|group| group.scope() == EvidenceScope::Package)
        .flat_map(|group| group.diagnostics().iter().cloned())
        .collect::<Vec<_>>();
    if package_wide
        .iter()
        .any(|diagnostic| diagnostic.severity() == crate::readme::Severity::Error)
    {
        return Err(OrderDiagnosisError::DamagedPackage {
            root: report.context().root().clone(),
            diagnostics: package_wide,
        });
    }

    // Presence in the Package's own discovery is what proves the scope exists: a healthy
    // Order produces no diagnostic group at all, and a directory that is not an entry of
    // the configured Orders Directory has no Order scope to report whatever stands under
    // it. The paths come from the same place for the same reason — they are the Package's
    // own account of where this Order's documents are.
    let Some(paths) = report
        .examined_orders()
        .iter()
        .find(|paths| paths.directory().absolute_path() == directory)
        .cloned()
    else {
        return Err(OrderDiagnosisError::OrderNotFound {
            directory: located_within(&root, &directory),
            root,
        });
    };
    let diagnostics = report
        .diagnostic_groups()
        .iter()
        .filter(|group| within_scope(group, &directory))
        .flat_map(|group| group.diagnostics().iter().cloned())
        .collect::<Vec<_>>();

    let valid = !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == crate::readme::Severity::Error);
    let (cases, unactionable) = cases_for(&diagnostics);

    Ok(OrderDiagnosis {
        paths,
        root,
        valid,
        diagnostics,
        cases,
        unactionable,
    })
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
fn package_root_of<F: crate::ports::FileSystem>(
    directory: &Path,
    filesystem: &F,
) -> Result<PathBuf, OrderDiagnosisError<F::Error>> {
    for candidate in directory.ancestors().skip(1) {
        let marker = candidate.join(".spielorders").join("version.json");
        match filesystem.symlink_metadata(&marker) {
            Ok(Some(_)) => return Ok(candidate.to_path_buf()),
            Ok(None) => {}
            Err(error) => {
                return Err(OrderDiagnosisError::FileSystem {
                    path: marker,
                    error,
                });
            }
        }
    }
    Err(OrderDiagnosisError::UnmanagedPackage {
        directory: directory.to_path_buf(),
    })
}

fn within_scope(group: &DiagnosticGroup, directory: &Path) -> bool {
    match group.scope() {
        EvidenceScope::Package => false,
        _ => group.location().absolute_path().starts_with(directory),
    }
}
