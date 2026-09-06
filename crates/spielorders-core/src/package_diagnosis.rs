//! Read-only validation of one Package's own scope, and the repair cases its evidence
//! authorizes.
//!
//! Diagnosis mutates nothing. It reuses the exhaustive Package validation the read model
//! already owns and keeps the Package-scope findings, so a Package-level answer and a
//! Package-wide one can never disagree about the same evidence or name the same condition
//! with two different codes.
//!
//! **Only this Package's own artifacts.** A finding inside an Order Directory belongs to
//! that Order's diagnosis, which reports it beside the cross-record evidence an Order
//! needs to be judged at all. A Package-level answer that reached into one would act on a
//! scope nobody named — and would decide, from the Package's vantage, a question the
//! Order's own scope already answers.

use crate::{
    canonical::{
        ExhaustivePackageValidation, ManagedContext, OperationFailedProjection, PackageIndexRecord,
        UnmanagedPackage, validate_package_exhaustively,
    },
    readme::{Diagnostic, EvidenceScope, Location, Severity},
    repair_cases::{RepairCase, UnactionableFinding, cases_for},
};
use std::path::Path;

/// The complete read-only result of validating one Package's own scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageDiagnosis {
    context: ManagedContext,
    valid: bool,
    diagnostics: Vec<Diagnostic>,
    cases: Vec<RepairCase>,
    unactionable: Vec<UnactionableFinding>,
    index: PackageIndexRecord,
}

impl PackageDiagnosis {
    /// This Package's managed locations, as the load established them.
    pub fn context(&self) -> &ManagedContext {
        &self.context
    }

    /// The SpielOrders Root this diagnosis was taken against.
    pub fn root(&self) -> &Location {
        self.context.root()
    }

    /// Whether this Package's own scope carries no error-severity finding.
    ///
    /// **Its own scope, not the whole Package.** A Damaged Order makes the *Package*
    /// invalid, which is what `validate` reports; it says nothing about whether this
    /// Package's own artifacts can be trusted, which is the question a Package-level
    /// repair answers.
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Every Package-scope diagnostic, in the established deterministic order, using the
    /// codes and evidence the Package-wide validation reports.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn cases(&self) -> &[RepairCase] {
        &self.cases
    }

    pub fn case(&self, handle: &str) -> Option<&RepairCase> {
        self.cases.iter().find(|case| case.handle() == handle)
    }

    /// Diagnosed conditions this core offers no safe action for. They are reported rather
    /// than silently omitted, so a caller can tell "nothing is wrong" from "nothing safe
    /// can be done from here".
    pub fn unactionable(&self) -> &[UnactionableFinding] {
        &self.unactionable
    }

    /// The derived index as the validating load observed it.
    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

#[derive(Debug, PartialEq)]
pub enum PackageDiagnosisError {
    /// The exact path given is a resolved directory and is not a SpielOrders Root.
    Unmanaged(UnmanagedPackage),
    /// The Package could not be read past its Root.
    OperationFailed(Box<OperationFailedProjection>),
}

/// Validates one Package's own scope and diagnoses the explicit safe repairs its evidence
/// authorizes.
pub fn diagnose_package<F: crate::ports::FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<PackageDiagnosis, PackageDiagnosisError> {
    let report = match validate_package_exhaustively(selected_root, filesystem) {
        ExhaustivePackageValidation::Complete(report) => report,
        ExhaustivePackageValidation::Unmanaged(package) => {
            return Err(PackageDiagnosisError::Unmanaged(package));
        }
        ExhaustivePackageValidation::OperationFailed(failed) => {
            return Err(PackageDiagnosisError::OperationFailed(Box::new(failed)));
        }
    };
    let diagnostics = report
        .diagnostic_groups()
        .iter()
        .filter(|group| group.scope() == EvidenceScope::Package)
        .flat_map(|group| group.diagnostics().iter().cloned())
        .collect::<Vec<_>>();
    let valid = !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == Severity::Error);
    let (cases, unactionable) = cases_for(&diagnostics);
    Ok(PackageDiagnosis {
        context: report.context().clone(),
        valid,
        diagnostics,
        cases,
        unactionable,
        index: report.index().clone(),
    })
}
