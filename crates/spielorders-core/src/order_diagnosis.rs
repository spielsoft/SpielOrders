//! Read-only scoped validation of exactly one Order, and the deterministic ephemeral
//! repair cases its evidence authorizes.
//!
//! Diagnosis mutates nothing. It reuses the exhaustive Package validation the read
//! model already owns and narrows the result to one Order, so a scoped diagnosis and
//! a Package-wide one can never disagree about the same evidence or name the same
//! condition with two different codes.

use crate::{
    canonical::{DiagnosticGroup, ExhaustivePackageValidation, validate_package_exhaustively},
    evidence::{EvidenceFingerprint, location_text},
    readme::{Diagnostic, DiagnosticCode, EvidenceScope, Location},
};
use std::path::{Path, PathBuf};

/// One explicit safe action a diagnosed case authorizes. Each is executed by a
/// separate capability; diagnosis only states which are available.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RepairAction {
    /// Accept one user-confirmed complete replacement for parseable metadata.
    ReplaceManagedRecord,
    /// Promote the exact diagnosed staging candidate.
    PromoteStagingCandidate,
    /// Point an unresolved Vendor Reference at an existing Vendor.
    AssignExistingVendor,
    /// Deliberately create one fully specified distinct Vendor and assign it.
    CreateAndAssignVendor,
    /// Clear the unresolved Vendor Reference outright.
    ClearVendorReference,
    /// Adopt a fresh identity for this side of a duplicate Order ID conflict.
    AdoptNewOrderIdentity,
    /// Recompute the technical locator from the physical Package relationship.
    RepairPackageLocator,
}

impl RepairAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReplaceManagedRecord => "replace_managed_record",
            Self::PromoteStagingCandidate => "promote_staging_candidate",
            Self::AssignExistingVendor => "assign_existing_vendor",
            Self::CreateAndAssignVendor => "create_and_assign_vendor",
            Self::ClearVendorReference => "clear_vendor_reference",
            Self::AdoptNewOrderIdentity => "adopt_new_order_identity",
            Self::RepairPackageLocator => "repair_package_locator",
        }
    }

    /// The resolution values a caller must supply to apply this action. An action
    /// with none is fully determined by the evidence already diagnosed.
    pub fn required_values(self) -> &'static [&'static str] {
        match self {
            Self::ReplaceManagedRecord => &["record"],
            Self::AssignExistingVendor => &["vendor_id"],
            Self::CreateAndAssignVendor => &["name"],
            Self::PromoteStagingCandidate
            | Self::ClearVendorReference
            | Self::AdoptNewOrderIdentity
            | Self::RepairPackageLocator => &[],
        }
    }
}

/// One deterministic ephemeral repair case. The handle identifies a case within one
/// diagnosis snapshot; it is not a domain identity and is never persisted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairCase {
    handle: String,
    code: DiagnosticCode,
    scope: EvidenceScope,
    location: Location,
    actions: Vec<RepairAction>,
}

impl RepairCase {
    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn code(&self) -> DiagnosticCode {
        self.code
    }

    pub fn scope(&self) -> EvidenceScope {
        self.scope
    }

    /// The exact evidence this case was diagnosed from.
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn actions(&self) -> &[RepairAction] {
        &self.actions
    }
}

/// One diagnosed condition for which no safe core action exists. Correcting it
/// requires work outside this core, after which validation can be run again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnactionableFinding {
    code: DiagnosticCode,
    location: Location,
}

impl UnactionableFinding {
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }

    pub fn location(&self) -> &Location {
        &self.location
    }
}

/// The complete read-only result of validating one selected Order scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderDiagnosis {
    directory: PathBuf,
    root: PathBuf,
    valid: bool,
    diagnostics: Vec<Diagnostic>,
    cases: Vec<RepairCase>,
    unactionable: Vec<UnactionableFinding>,
}

impl OrderDiagnosis {
    /// The exact Order Directory this diagnosis examined.
    pub fn directory(&self) -> &Path {
        &self.directory
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
        self.cases.iter().find(|case| case.handle == handle)
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
    UnmanagedPackage,
    /// Package-wide state is untrustworthy, so an Order-scoped answer would be
    /// misleading. A Package-wide failure is never downgraded to an Order finding.
    DamagedPackage,
    PackageOperationFailed,
    /// The selected directory is not an entry of this Package's Orders Directory.
    OrderNotFound {
        path: PathBuf,
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
            return Err(OrderDiagnosisError::UnmanagedPackage);
        }
        ExhaustivePackageValidation::OperationFailed(_) => {
            return Err(OrderDiagnosisError::PackageOperationFailed);
        }
    };

    // A Package-wide failure is never downgraded to an Order finding. Answering
    // "this Order looks fine" while the Package it belongs to is untrustworthy would
    // be worse than refusing, because the Order-scoped answer depends on
    // package-wide evidence such as the Vendor aggregate.
    if report.diagnostic_groups().iter().any(|group| {
        group.scope() == EvidenceScope::Package
            && group
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.severity() == crate::readme::Severity::Error)
    }) {
        return Err(OrderDiagnosisError::DamagedPackage);
    }

    let mut diagnostics = Vec::new();
    let mut examined = false;
    for group in report.diagnostic_groups() {
        if !within_scope(group, &directory) {
            continue;
        }
        examined = true;
        diagnostics.extend(group.diagnostics().iter().cloned());
    }
    // A healthy Order produces no group at all, so presence in the Package's own
    // discovery is what proves the scope exists.
    if !examined && !discovered(&report, &directory) {
        return Err(OrderDiagnosisError::OrderNotFound { path: directory });
    }

    let valid = !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == crate::readme::Severity::Error);
    let (cases, unactionable) = cases_for(&diagnostics);

    Ok(OrderDiagnosis {
        directory,
        root,
        valid,
        diagnostics,
        cases,
        unactionable,
    })
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
    Err(OrderDiagnosisError::UnmanagedPackage)
}

fn within_scope(group: &DiagnosticGroup, directory: &Path) -> bool {
    match group.scope() {
        EvidenceScope::Package => false,
        _ => group.location().absolute_path().starts_with(directory),
    }
}

fn discovered(report: &crate::canonical::ExhaustiveValidationReport, directory: &Path) -> bool {
    report.order_readmes().iter().any(|readme| {
        readme
            .reference()
            .location()
            .absolute_path()
            .starts_with(directory)
    })
}

/// Maps diagnosed evidence to the explicit safe actions the repair capabilities are
/// authorized to execute. An evidence family absent from this mapping deliberately
/// has no case: this core never guesses a fact, salvages malformed JSON, interprets
/// an unsupported version, or adopts missing metadata.
fn cases_for(diagnostics: &[Diagnostic]) -> (Vec<RepairCase>, Vec<UnactionableFinding>) {
    let mut cases = Vec::new();
    let mut unactionable = Vec::new();
    for diagnostic in diagnostics {
        // Whether a safe action exists follows from the evidence family, not from
        // severity. An abandoned staging candidate does not invalidate its Order, yet
        // promoting it is one of the explicit repairs this core is authorized to
        // offer.
        let actions: &[RepairAction] = match diagnostic.code() {
            // Parseable metadata that fails its schema or Package semantics may be
            // replaced wholesale by a confirmed valid record.
            DiagnosticCode::ManagedArtifactInvalidShape | DiagnosticCode::OrderSemanticInvalid => {
                &[RepairAction::ReplaceManagedRecord]
            }
            DiagnosticCode::StagingArtifactPresent => &[RepairAction::PromoteStagingCandidate],
            DiagnosticCode::VendorReferenceUnresolved => &[
                RepairAction::AssignExistingVendor,
                RepairAction::CreateAndAssignVendor,
                RepairAction::ClearVendorReference,
            ],
            DiagnosticCode::OrderIdDuplicate => &[RepairAction::AdoptNewOrderIdentity],
            DiagnosticCode::OrderPackageLocatorMismatch => &[RepairAction::RepairPackageLocator],
            _ => &[],
        };
        if actions.is_empty() {
            // Only a condition that actually invalidates the Order is reported as
            // one nothing safe can be done about; an ordinary warning is simply a
            // diagnostic.
            if diagnostic.severity() == crate::readme::Severity::Error {
                unactionable.push(UnactionableFinding {
                    code: diagnostic.code(),
                    location: diagnostic.primary_location().clone(),
                });
            }
            continue;
        }
        let location = diagnostic.primary_location().clone();
        cases.push(RepairCase {
            handle: handle_for(diagnostic),
            code: diagnostic.code(),
            scope: diagnostic.scope(),
            location,
            actions: actions.to_vec(),
        });
    }
    (cases, unactionable)
}

/// A handle names one diagnosed condition *and the evidence that distinguishes it*.
///
/// The code and the path are kept because they make a handle recognizable, but they
/// are not what identifies the condition: both are identical across every candidate
/// that has ever occupied one path, so a handle built from them alone confirms a
/// repair of whatever happens to be there now. The evidence digest is what makes a
/// confirmation stop working once the fact it named has moved.
///
/// The evidence comes from [`EvidenceFingerprint::of_diagnostic`] rather than being
/// restated here, so exactly one place decides which facts distinguish a diagnosed
/// condition, and a handle and a plan confirmed against the same diagnosis cannot
/// disagree about it.
///
/// It stays derived, so the same snapshot always yields the same handle and a
/// corrected condition simply stops appearing.
fn handle_for(diagnostic: &Diagnostic) -> String {
    let path = location_text(diagnostic.primary_location());
    format!(
        "{}:{path}:{}",
        diagnostic.code().as_str(),
        EvidenceFingerprint::of_diagnostic(diagnostic).digest()
    )
}
