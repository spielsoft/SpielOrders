//! The one vocabulary for what may be done about a diagnosed finding.
//!
//! Two scopes ask this question — a Package's own artifacts and one Order's — and they
//! ask it once. Which conditions authorize a safe action, what each action is called, and
//! what a caller must supply to apply one are repair-boundary rules, so a scope deciding
//! them again would be a second place deciding what this core may do.
//!
//! Nothing here performs a repair. Diagnosis states which actions are available; the
//! capabilities that execute them are `metadata_repair`, `semantic_repair`, and
//! `package_repair`.

use crate::{
    evidence::{EvidenceFingerprint, location_text},
    readme::{Diagnostic, DiagnosticCode, Location, Severity},
};

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
    /// Every action this vocabulary has, each exactly once.
    ///
    /// Listed because an array cannot be derived from an enum, and needed because the
    /// spelling has to be read as well as written: a command line naming an action is
    /// resolved through [`Self::parse`], so an adapter never carries a second table of
    /// these words.
    pub const ALL: [Self; 7] = [
        Self::ReplaceManagedRecord,
        Self::PromoteStagingCandidate,
        Self::AssignExistingVendor,
        Self::CreateAndAssignVendor,
        Self::ClearVendorReference,
        Self::AdoptNewOrderIdentity,
        Self::RepairPackageLocator,
    ];

    /// The action this text names, or `None` where it names none.
    ///
    /// The inverse of [`Self::as_str`] and beside it, so one action is spelled one way in
    /// a diagnosis, in a response, and on the command line that applies it.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.as_str() == text)
    }

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
            Self::AssignExistingVendor => &["vendor"],
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

/// The explicit safe actions this core is authorized to execute for one diagnosed
/// condition, or none where no safe action exists.
///
/// **Asked here rather than re-derived by a caller.** Which conditions have a safe
/// action is a repair-boundary rule, and a surface that decided it again — to say
/// whether a finding is worth suggesting a command for, say — would be a second place
/// deciding what this core may do. A code absent from this mapping deliberately has no
/// case: this core never guesses a fact, salvages malformed JSON, interprets an
/// unsupported version, or adopts missing metadata.
pub fn repair_actions(code: DiagnosticCode) -> &'static [RepairAction] {
    match code {
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
    }
}

/// Maps diagnosed evidence to the explicit safe actions the repair capabilities are
/// authorized to execute.
pub(crate) fn cases_for(diagnostics: &[Diagnostic]) -> (Vec<RepairCase>, Vec<UnactionableFinding>) {
    let mut cases = Vec::new();
    let mut unactionable = Vec::new();
    for diagnostic in diagnostics {
        // Whether a safe action exists follows from the evidence family, not from
        // severity. An abandoned staging candidate does not invalidate its Order, yet
        // promoting it is one of the explicit repairs this core is authorized to
        // offer.
        let actions = repair_actions(diagnostic.code());
        if actions.is_empty() {
            // Only a condition that actually invalidates the Order is reported as
            // one nothing safe can be done about; an ordinary warning is simply a
            // diagnostic.
            if diagnostic.severity() == Severity::Error {
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
