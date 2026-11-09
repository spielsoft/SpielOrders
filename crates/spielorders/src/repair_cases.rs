//! The one vocabulary for what may be done about a diagnosed finding.
//!
//! Two scopes ask this question — a Package's own artifacts and one Order's — and they
//! ask it once. Which conditions authorize a safe action, what each action is called, and
//! what a caller must supply to apply one are repair-boundary rules, so a scope deciding
//! them again would be a second place deciding what this core may do.
//!
//! Nothing here performs a repair. Diagnosis states which actions are available; the
//! capability that executes them is `repair`.

use crate::code::Coded;
use crate::{
    Assignments, Failure,
    evidence::{EvidenceFingerprint, location_text},
    readme::{Diagnostic, DiagnosticCode, Location},
};
use serde::Serialize;

/// One explicit safe action a diagnosed case authorizes. Each is executed by a
/// separate capability; diagnosis only states which are available.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
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
    pub(crate) const ALL: [Self; 7] = [
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
    /// The inverse of this action's code and beside it, so one action is spelled one way in
    /// a diagnosis, in a response, and on the command line that applies it.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.code() == text)
    }

    /// The resolution values a caller must supply to apply this action. An action
    /// with none is fully determined by the evidence already diagnosed.
    pub(crate) fn required_values(self) -> &'static [&'static str] {
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

impl crate::code::Coded for RepairAction {}

/// One action applied to one diagnosed case, carrying exactly the values that action
/// takes. The action and what resolves it are one value, so a confirmation of one action
/// carrying another's values cannot be built. Nothing here is inferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RepairResolution {
    /// The complete replacement artifact, exactly as the user confirmed it. This core
    /// neither composes nor edits it.
    ReplaceManagedRecord {
        record: Vec<u8>,
    },
    /// The diagnosed candidate is the only one that may be promoted, so promotion takes
    /// no value.
    PromoteStagingCandidate,
    /// Point the unresolved reference at the existing Vendor this Vendor Name selects.
    ///
    /// The Name resolves as every Vendor Name does, and the repaired record stores the
    /// Vendor ID it resolves to.
    AssignExistingVendor {
        name: String,
    },
    /// Deliberately create one distinct Vendor with this Name and assign it. This is a
    /// new Vendor, never a merge with or a redirection to an existing one.
    CreateAndAssignVendor {
        name: String,
    },
    ClearVendorReference,
    /// Adopt a fresh identity for this Order, which the caller has explicitly chosen as
    /// the non-surviving side of the conflict.
    AdoptNewOrderIdentity,
    RepairPackageLocator,
}

impl RepairResolution {
    /// The resolution of `action` that `resolutions` spell, by the names
    /// [`RepairAction::required_values`] gives them.
    pub(crate) fn from_assignments(
        action: RepairAction,
        resolutions: &Assignments,
    ) -> Result<Self, Failure> {
        let resolutions = resolutions.pairs();
        let required = action.required_values();
        let value = |name: &str| {
            resolutions
                .iter()
                .find(|(field, value)| field == name && !value.trim().is_empty())
                .map(|(_, value)| value.clone())
        };
        let missing: Vec<&'static str> = required
            .iter()
            .copied()
            .filter(|name| value(name).is_none())
            .collect();
        if !missing.is_empty() {
            return Err(Failure::RepairResolutionMissing { action, missing });
        }
        if let Some((field, _)) = resolutions
            .iter()
            .find(|(field, _)| !required.contains(&field.as_str()))
        {
            return Err(Failure::RepairResolutionUnexpected {
                action,
                field: field.clone(),
            });
        }
        let resolved = |name| value(name).unwrap_or_default();
        Ok(match action {
            RepairAction::ReplaceManagedRecord => Self::ReplaceManagedRecord {
                record: resolved("record").into_bytes(),
            },
            RepairAction::PromoteStagingCandidate => Self::PromoteStagingCandidate,
            RepairAction::AssignExistingVendor => Self::AssignExistingVendor {
                name: resolved("vendor"),
            },
            RepairAction::CreateAndAssignVendor => Self::CreateAndAssignVendor {
                name: resolved("name"),
            },
            RepairAction::ClearVendorReference => Self::ClearVendorReference,
            RepairAction::AdoptNewOrderIdentity => Self::AdoptNewOrderIdentity,
            RepairAction::RepairPackageLocator => Self::RepairPackageLocator,
        })
    }

    pub fn action(&self) -> RepairAction {
        match self {
            Self::ReplaceManagedRecord { .. } => RepairAction::ReplaceManagedRecord,
            Self::PromoteStagingCandidate => RepairAction::PromoteStagingCandidate,
            Self::AssignExistingVendor { .. } => RepairAction::AssignExistingVendor,
            Self::CreateAndAssignVendor { .. } => RepairAction::CreateAndAssignVendor,
            Self::ClearVendorReference => RepairAction::ClearVendorReference,
            Self::AdoptNewOrderIdentity => RepairAction::AdoptNewOrderIdentity,
            Self::RepairPackageLocator => RepairAction::RepairPackageLocator,
        }
    }
}

/// One deterministic ephemeral repair case. The handle identifies a case within one
/// diagnosis snapshot; it is not a domain identity and is never persisted. It carries
/// each action with the values a caller must supply to apply it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairCase {
    pub handle: String,
    pub code: DiagnosticCode,
    /// The exact evidence this case was diagnosed from.
    pub location: Location,
    pub actions: Vec<RepairAction>,
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
pub(crate) fn cases_for(diagnostics: &[Diagnostic]) -> Vec<RepairCase> {
    let mut cases = Vec::new();
    for diagnostic in diagnostics {
        // Whether a safe action exists follows from the evidence family, not from
        // severity. An abandoned staging candidate does not invalidate its Order, yet
        // promoting it is one of the explicit repairs this core is authorized to
        // offer.
        let actions = repair_actions(diagnostic.code);
        if actions.is_empty() {
            continue;
        }
        let location = diagnostic.primary_location.clone();
        cases.push(RepairCase {
            handle: handle_for(diagnostic),
            code: diagnostic.code,
            location,
            actions: actions.to_vec(),
        });
    }
    cases
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
    let path = location_text(&diagnostic.primary_location);
    format!(
        "{}:{path}:{}",
        diagnostic.code.code(),
        EvidenceFingerprint::of_diagnostic(diagnostic).digest()
    )
}
