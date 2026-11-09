//! Binding a mutation plan to the exact evidence that authorized it.
//!
//! A plan is authorized against facts observed at planning time. Between then and the
//! first effect those facts can change, and checking only that the subject still
//! *exists* does not notice: a repair handle naming a diagnostic code and a path is
//! identical whether the unresolved Vendor is the one the user confirmed or a
//! different one, and a directory is still a directory after it has been replaced by
//! a different Order.
//!
//! This module defines what a plan records, and the seam a capability calls
//! immediately before its first effect. It performs no I/O and knows nothing about any
//! capability: the caller decides which facts it relied on and re-observes them.

use crate::code::Coded;
use crate::digest::{ContentDigest, DigestParts};
use crate::readme::{Diagnostic, DiagnosticEvidence};
use std::collections::BTreeMap;

/// One fact a plan relied on, as it stood when the plan was built.
///
/// A fact is either a value the plan observed, or the plan's observation that nothing
/// was there. The distinction matters: "no Vendor Reference" and "a Vendor Reference
/// that happens to be empty" authorize different repairs.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Fact {
    Observed(Vec<u8>),
    Absent,
}

/// A deterministic fingerprint of the facts a plan relied on.
///
/// It stores the facts rather than a digest of them, because a lossy fingerprint would
/// treat two conditions it cannot tell apart as the same. Nothing here is written to disk,
/// so the cost is bounded by what one plan relied on. [`digest`](Self::digest) renders
/// one of these as a bounded *name*; no comparison ever goes through it.
///
/// Facts are keyed by name, so two plans that record the same facts agree regardless
/// of the order they recorded them in.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct EvidenceFingerprint {
    facts: BTreeMap<String, Fact>,
}

impl EvidenceFingerprint {
    /// Starts an empty fingerprint. A plan adds every fact it relied on.
    ///
    /// An empty fingerprint never confirms: see [`EvidenceOutcome::NoEvidence`].
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records a fact the plan observed. Re-recording the same name replaces it, so a
    /// caller refining an observation cannot leave a stale one behind.
    #[must_use]
    pub(crate) fn observed(mut self, name: &str, value: impl AsRef<[u8]>) -> Self {
        self.facts
            .insert(name.to_owned(), Fact::Observed(value.as_ref().to_vec()));
        self
    }

    /// Records that the plan observed nothing at this name.
    #[must_use]
    pub(crate) fn absent(mut self, name: &str) -> Self {
        self.facts.insert(name.to_owned(), Fact::Absent);
        self
    }

    /// A stable, bounded name for exactly this set of facts.
    ///
    /// [`confirm`](Self::confirm) compares the facts themselves, never this lossy digest.
    /// This serves the other job — referring to one set of facts — where an unbounded pile
    /// of evidence is not usable as a name.
    /// Every recorded fact participates, so a name derived from this cannot outlive
    /// the evidence it was derived from.
    ///
    /// Names and values are fed as length-prefixed parts, so no two different sets of
    /// facts share a byte stream.
    #[must_use]
    pub(crate) fn digest(&self) -> ContentDigest {
        let mut parts = DigestParts::new();
        for (name, fact) in &self.facts {
            parts.part(name.as_bytes());
            match fact {
                Fact::Observed(value) => {
                    parts.part(b"observed").part(value);
                }
                Fact::Absent => {
                    parts.part(b"absent");
                }
            }
        }
        parts.finish()
    }

    /// Compares the evidence this plan was authorized against with the evidence as it
    /// stands now. A capability re-observes the same facts
    /// immediately before its first effect and calls this.
    #[must_use = "the whole purpose of this seam is to refuse before an effect; \
                  ignoring the outcome performs the effect unconditionally"]
    pub(crate) fn confirm(&self, current: &Self) -> EvidenceOutcome {
        // A plan that recorded nothing has authorized nothing. Answering "unchanged"
        // here would be the same empty reassurance as "the path still exists".
        if self.facts.is_empty() {
            return EvidenceOutcome::NoEvidence;
        }
        let mut changes = Vec::new();
        for (name, planned) in &self.facts {
            match current.facts.get(name) {
                Some(now) if now == planned => {}
                Some(Fact::Absent) => changes.push(ChangedFact {
                    name: name.clone(),
                    change: FactChange::Vanished,
                }),
                Some(Fact::Observed(_)) => changes.push(ChangedFact {
                    name: name.clone(),
                    change: match planned {
                        Fact::Absent => FactChange::Appeared,
                        Fact::Observed(_) => FactChange::Differs,
                    },
                }),
                // The capability did not re-observe this fact at all, so nothing
                // confirms it. That is not evidence it still holds.
                None => changes.push(ChangedFact {
                    name: name.clone(),
                    change: FactChange::Unobserved,
                }),
            }
        }
        // A fact the plan never relied on is not a reason to refuse: the plan was not
        // authorized against it, so its appearance changes nothing this plan claimed.
        if changes.is_empty() {
            EvidenceOutcome::Unchanged
        } else {
            EvidenceOutcome::Changed(StaleEvidence { changed: changes })
        }
    }
}

/// The facts a diagnosis actually carries, as a fingerprint.
///
/// A repair is confirmed against a `Diagnostic`, and this derives the plan's evidence
/// from that diagnosis rather than leaving an adopter to guess which facts distinguish
/// it: a handle naming only the code and the path is identical across two different
/// unresolved Vendors.
///
/// The whole evidence payload is recorded, so any field differing produces a different
/// fingerprint. The fields that distinguish the repair cases are *also* recorded under
/// their own names, so a refusal can say `unresolved_vendor_id` rather than pointing at
/// an opaque blob.
///
/// # What this cannot cover
///
/// A staged candidate with no readable content — a directory or a link at the staging
/// name, or a file that cannot be read — has nothing that tells it apart from another
/// in the same state, so `staging_content` is recorded absent rather than given a
/// stand-in value. Two such candidates do confirm against each other; the repair that
/// follows then fails on the same unreadable content, rather than writing anything.
impl EvidenceFingerprint {
    /// Derives the evidence a repair plan relies on from the diagnosis that authorized
    /// it.
    #[must_use]
    pub(crate) fn of_diagnostic(diagnostic: &Diagnostic) -> Self {
        let fingerprint = Self::new()
            .observed("diagnostic_code", diagnostic.code.code())
            .observed(
                "diagnostic_location",
                location_text(&diagnostic.primary_location),
            )
            // Every field of the payload, so nothing differing can slip through the
            // named facts below.
            .observed("diagnostic_evidence", format!("{:?}", diagnostic.evidence));
        // The fields that distinguish one repair case from another at the same code
        // and path, named so a refusal can say which one moved.
        match &diagnostic.evidence {
            DiagnosticEvidence::VendorReferenceUnresolved {
                order_id,
                vendor_id,
            } => fingerprint
                .observed("subject_order_id", order_id.as_str())
                .observed("unresolved_vendor_id", vendor_id.as_str()),
            DiagnosticEvidence::OrderIdDuplicate {
                order_id,
                conflict_locations,
            } => fingerprint
                .observed("subject_order_id", order_id.as_str())
                .observed(
                    "duplicate_participants",
                    conflict_locations
                        .iter()
                        .map(location_text)
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            DiagnosticEvidence::VendorIdDuplicate { vendor_id } => {
                fingerprint.observed("duplicate_vendor_id", vendor_id.as_str())
            }
            DiagnosticEvidence::VendorNameDuplicate { vendor_ids, .. } => fingerprint.observed(
                "duplicate_vendor_ids",
                vendor_ids
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            // Which candidate is staged, so promoting the confirmed one cannot promote
            // whatever replaced it. Absent when the diagnosis found no content that
            // identifies it, rather than a value that would confirm anything.
            DiagnosticEvidence::StagingArtifactPresent {
                candidate_digest, ..
            } => match candidate_digest {
                Some(digest) => fingerprint.observed("staging_content", digest.as_str()),
                None => fingerprint.absent("staging_content"),
            },
            _ => fingerprint,
        }
    }
}

/// A Location rendered stably: Package-relative where that is established, absolute
/// otherwise. Two runs over the same Package agree.
///
/// Shared with the repair handles derived from these fingerprints, so the name of a
/// condition and the evidence recorded for it cannot spell the same path two ways.
pub(crate) fn location_text(location: &crate::readme::Location) -> String {
    location
        .package_relative_path
        .clone()
        .unwrap_or_else(|| location.absolute_path.to_string_lossy().into_owned())
}

/// What happened to one fact between planning and the check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactChange {
    /// The plan observed a value, and the value there now is a different one.
    Differs,
    /// The plan observed a value and nothing is there now.
    Vanished,
    /// The plan observed that nothing was there, and something is there now. The
    /// mirror of `Vanished`, and not the same as `Differs`: a repair authorized
    /// against "there is no staging artifact" must not proceed once one exists.
    Appeared,
    /// The caller did not re-observe this fact, so nothing confirms it.
    Unobserved,
}

/// One fact that no longer stands as the plan recorded it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedFact {
    pub name: String,
    pub change: FactChange,
}

/// The evidence a plan was authorized against no longer holds.
///
/// This names what changed. A caller refusing on this can tell the user which fact
/// moved underneath them, rather than reporting an unexplained precondition failure —
/// and it is a distinct outcome from a write that failed and from a subject that was
/// never there.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaleEvidence {
    /// Every fact that no longer stands, in deterministic order.
    pub changed: Vec<ChangedFact>,
}

/// The result of confirming a plan's evidence immediately before its first effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvidenceOutcome {
    /// Every fact the plan relied on still stands. The effect may proceed.
    Unchanged,
    /// At least one fact moved. The effect must not proceed.
    Changed(StaleEvidence),
    /// The plan recorded no evidence at all, so nothing about it can be confirmed.
    /// Reported separately from `Changed` because the remedy is different: the plan
    /// is not stale, it was never bound to anything.
    NoEvidence,
}

impl EvidenceOutcome {
    pub(crate) fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged)
    }
}
