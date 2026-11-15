//! Entries: the one reader for failures, warnings, and findings, each with a code, a
//! severity, a message, and where it is.

use crate::interface::{
    SPIELORDERS_FAMILY_CONTACT_FIELD, SPIELORDERS_FAMILY_FAILURE, SPIELORDERS_FAMILY_FINDING,
    SPIELORDERS_FAMILY_ITEM_FIELD, SPIELORDERS_FAMILY_ORDER_FIELD, SPIELORDERS_FAMILY_REASON,
    SPIELORDERS_FAMILY_TEXT_FIELD, SPIELORDERS_FAMILY_VENDOR_FIELD, SPIELORDERS_FAMILY_WARNING,
    SPIELORDERS_SEVERITY_ERROR, SPIELORDERS_SEVERITY_WARNING, SpielordersConstant, Text,
    call::quietly, library::member, records::path_text,
};
use crate::{
    Coded, Failure, Warning,
    model::{
        Diagnostic, DiagnosticEvidence, EvidenceOutcome, FactChange, FailedPath, Location, Severity,
    },
};
use std::ffi::CString;

/// Text the interface hands out: the string up to any NUL it holds.
pub(crate) fn c_text(text: impl Into<Vec<u8>>) -> CString {
    let mut bytes = text.into();
    if let Some(nul) = bytes.iter().position(|byte| *byte == 0) {
        bytes.truncate(nul);
    }
    CString::new(bytes).unwrap_or_default()
}

/// The text `C""` an accessor answers for absent or misuse.
pub(crate) const EMPTY: &std::ffi::CStr = c"";

/// A Location's Package-relative spelling, or its absolute path where it has none.
pub(crate) fn relative(location: &Location) -> CString {
    match &location.package_relative_path {
        Some(relative) => c_text(relative.as_str()),
        None => path_text(&location.absolute_path),
    }
}

/// What a located field belongs to, so a field's name reads as the constant of its
/// record's family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Subject {
    Order,
    Vendor,
    Text,
    Other,
}

impl Subject {
    /// The field constant `name` spells for this subject, at a record's own facts or at
    /// one of its rows.
    pub(crate) fn field(self, name: &str, row: bool) -> SpielordersConstant {
        let families: &[SpielordersConstant] = match (self, row) {
            (Self::Order, false) => &[SPIELORDERS_FAMILY_ORDER_FIELD],
            (Self::Order, true) => &[SPIELORDERS_FAMILY_ITEM_FIELD],
            (Self::Vendor, false) => &[SPIELORDERS_FAMILY_VENDOR_FIELD],
            (Self::Vendor, true) => &[SPIELORDERS_FAMILY_CONTACT_FIELD],
            (Self::Text, _) => &[SPIELORDERS_FAMILY_TEXT_FIELD],
            (Self::Other, false) => &[
                SPIELORDERS_FAMILY_ORDER_FIELD,
                SPIELORDERS_FAMILY_VENDOR_FIELD,
            ],
            (Self::Other, true) => &[
                SPIELORDERS_FAMILY_ITEM_FIELD,
                SPIELORDERS_FAMILY_CONTACT_FIELD,
            ],
        };
        families
            .iter()
            .find_map(|family| member(*family, name))
            .unwrap_or(SpielordersConstant::NONE)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub(crate) code: SpielordersConstant,
    pub(crate) severity: SpielordersConstant,
    pub(crate) message: CString,
    pub(crate) field: SpielordersConstant,
    pub(crate) row: i64,
    pub(crate) path: CString,
    pub(crate) before: CString,
    pub(crate) after: CString,
    pub(crate) reason: SpielordersConstant,
}

/// The words a code spells, as a message a caller may show.
fn sentence(code: &str, detail: Option<&str>) -> CString {
    let mut text = code.replace('_', " ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
        text.push_str(": ");
        text.push_str(detail);
    }
    c_text(text)
}

fn located(path: &FailedPath) -> Option<CString> {
    Some(match path {
        FailedPath::Located(location) => relative(location),
        FailedPath::Unlocated(path) => path_text(path),
    })
}

/// The field and the place `failure` is about, where it names them.
fn subject_of(failure: &Failure) -> (Option<&str>, Option<CString>) {
    match failure {
        Failure::RootIsALink { root } => (None, located(root)),
        Failure::UnmanagedPackage(package) => (None, Some(relative(&package.root))),
        Failure::PackageAccessDenied(failed) | Failure::PackageOperationFailed(failed) => {
            (None, located(&failed.path))
        }
        Failure::NotAnOrderDirectory { directory, .. }
        | Failure::OrderNotFound { directory, .. }
        | Failure::LocatorUnusable { directory }
        | Failure::LocatorReachesNoPackageHoldingThisOrder { directory, .. } => {
            (None, located(directory))
        }
        Failure::DamagedRecord { directory, field } => (field.as_deref(), located(directory)),
        Failure::PackageHasNoVendors { context, .. } => (None, Some(relative(&context.root))),
        Failure::UnknownVendor { scope, .. }
        | Failure::VendorNameInUse { scope, .. }
        | Failure::MissingRequiredName { scope }
        | Failure::UnknownCase { scope, .. }
        | Failure::UnauthorizedAction { scope, .. } => (None, Some(relative(&scope.location))),
        Failure::UnrecognizedArtifact {
            destination: rooted,
        }
        | Failure::InvalidCandidate { candidate: rooted }
        | Failure::CandidateDamagesPackage {
            candidate: rooted, ..
        }
        | Failure::StagingEvidencePresent { staged: rooted }
        | Failure::EvidenceChanged { evidence: rooted } => (None, Some(relative(&rooted.location))),
        Failure::DestinationCollision { location, .. }
        | Failure::StagingCollision { location }
        | Failure::NotDirectChild { location }
        | Failure::OrderIdentityInUse { location, .. }
        | Failure::NameUnusableCandidate { location }
        | Failure::AlreadyAnOrder { location }
        | Failure::ConflictingReservedState { location }
        | Failure::PreconditionsChanged { location }
        | Failure::AlreadyAPackage { root: location }
        | Failure::InvalidConfiguredPath { root: location, .. }
        | Failure::PlannedPathCollision { path: location, .. }
        | Failure::DisallowedLink { path: location, .. }
        | Failure::UnwritableDestination { path: location, .. } => (None, Some(relative(location))),
        Failure::ManagedPathsOverlap { root } => (None, Some(relative(root))),
        Failure::OrderReadmeTextUnavailable(package) => {
            (None, Some(relative(&package.context.root)))
        }
        Failure::CommitFailed { failure }
        | Failure::CommitFailedCleanupIncomplete { failure, .. } => {
            (None, Some(relative(&failure.location)))
        }
        Failure::IndexReplacementFailed(failure) => subject_of(failure),
        Failure::RepairResolutionUnexpected { field, .. } => (Some(field), None),
        _ => (None, None),
    }
}

/// What a stopped commit's cleanup left behind, wherever `failure` carries one.
fn left_behind(failure: &Failure) -> &[crate::mutation::CleanupFailure] {
    match failure {
        Failure::CommitFailedCleanupIncomplete {
            cleanup_failures, ..
        } => cleanup_failures,
        Failure::IndexReplacementFailed(failure) => left_behind(failure),
        _ => &[],
    }
}

impl Entry {
    pub(crate) fn new(
        code: SpielordersConstant,
        severity: SpielordersConstant,
        message: CString,
    ) -> Self {
        Self {
            code,
            severity,
            message,
            field: SpielordersConstant::NONE,
            row: -1,
            path: CString::default(),
            before: CString::default(),
            after: CString::default(),
            reason: SpielordersConstant::NONE,
        }
    }

    fn coded(family: SpielordersConstant, code: &str, detail: Option<&str>) -> Self {
        Self::new(
            member(family, code).unwrap_or(SpielordersConstant::NONE),
            SPIELORDERS_SEVERITY_ERROR,
            sentence(code, detail),
        )
    }

    pub(crate) fn finding(diagnostic: &Diagnostic) -> Self {
        let code = diagnostic.code.code();
        let mut entry = Self::coded(SPIELORDERS_FAMILY_FINDING, code, None);
        entry.severity = match diagnostic.severity {
            Severity::Error => SPIELORDERS_SEVERITY_ERROR,
            Severity::Warning => SPIELORDERS_SEVERITY_WARNING,
        };
        entry.path = relative(&diagnostic.primary_location);
        let (field, value) = match &diagnostic.evidence {
            DiagnosticEvidence::ManagedArtifact { field, .. } => (field.as_deref(), None),
            DiagnosticEvidence::OrderSemanticInvalid {
                field, order_id, ..
            } => (Some(field.as_str()), Some(order_id.as_str())),
            DiagnosticEvidence::VendorIdDuplicate { vendor_id }
            | DiagnosticEvidence::VendorReferenceUnresolved { vendor_id, .. } => {
                (None, Some(vendor_id.as_str()))
            }
            DiagnosticEvidence::OrderPackageLocatorMismatch { order_id, .. }
            | DiagnosticEvidence::OrderIdDuplicate { order_id, .. } => {
                (None, Some(order_id.as_str()))
            }
            DiagnosticEvidence::ConfigurationPathInvalid {
                configured_value, ..
            }
            | DiagnosticEvidence::ConfiguredOrdersDirectoryUnavailable {
                configured_value, ..
            }
            | DiagnosticEvidence::ConfiguredOrdersDirectoryNameUnusable { configured_value } => {
                (None, Some(configured_value.as_str()))
            }
            _ => (None, None),
        };
        if let Some(field) = field {
            entry.field = Subject::Other.field(field, false);
        }
        if let Some(value) = value {
            entry.after = c_text(value);
        }
        entry
    }

    pub(crate) fn warning(warning: &Warning) -> Self {
        let mut entry = Self::coded(SPIELORDERS_FAMILY_WARNING, warning.code(), None);
        entry.severity = SPIELORDERS_SEVERITY_WARNING;
        let location = match warning {
            Warning::IndexRefreshFailed(failure) | Warning::LocatorUpdateFailed(failure) => {
                subject_of(failure).1
            }
            Warning::StagingCleanupFailed(failure) => Some(relative(&failure.location)),
            Warning::RefreshUnavailable | Warning::RediagnosisUnavailable => None,
        };
        if let Some(path) = location {
            entry.path = path;
        }
        entry
    }

    /// The entries `warning` makes: one for the warning, then one beside it for each
    /// artifact the failed step it reports left behind.
    pub(crate) fn warnings(warning: &Warning, entries: &mut Vec<Self>) {
        let entry = Self::warning(warning);
        let code = entry.code;
        entries.push(entry);
        if let Warning::IndexRefreshFailed(failure) | Warning::LocatorUpdateFailed(failure) =
            warning
        {
            entries.extend(left_behind(failure).iter().map(|left| {
                let mut entry = Self::new(
                    code,
                    SPIELORDERS_SEVERITY_WARNING,
                    sentence(warning.code(), Some("left behind")),
                );
                entry.path = relative(&left.location);
                entry
            }));
        }
    }

    /// The entries `failure` makes: one for the failure, one more for each fact, row,
    /// or finding it carries.
    pub(crate) fn failures(failure: &Failure, subject: Subject, entries: &mut Vec<Self>) {
        Self::located(failure, subject, -1, entries);
    }

    fn located(failure: &Failure, subject: Subject, row: i64, entries: &mut Vec<Self>) {
        let code = failure.code();
        let at = |mut entry: Self, field: &str| {
            entry.row = row;
            entry.field = subject.field(field, row >= 0);
            entry
        };
        match failure {
            Failure::InvalidDraft(faults) => {
                for fault in faults {
                    let row = fault
                        .row
                        .and_then(|row| i64::try_from(row).ok())
                        .map_or(-1, |row| row - 1);
                    Self::located(&fault.failure, subject, row, entries);
                }
            }
            Failure::InvalidAssignments(rejected) => {
                entries.extend(rejected.iter().map(|rejected| {
                    let reason = rejected.reason.code();
                    let mut entry = at(
                        Self::coded(
                            SPIELORDERS_FAMILY_FAILURE,
                            code,
                            Some(&reason.replace('_', " ")),
                        ),
                        &rejected.field,
                    );
                    entry.reason = member(SPIELORDERS_FAMILY_REASON, reason)
                        .unwrap_or(SpielordersConstant::NONE);
                    entry
                }));
            }
            Failure::MissingRequiredFacts { fields }
            | Failure::MissingRequiredItemFacts { fields } => {
                entries.extend(
                    fields.iter().map(|field| {
                        at(Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None), field)
                    }),
                );
            }
            Failure::UnknownValue { field, value } => {
                let mut entry = at(Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None), field);
                entry.after = c_text(value.as_str());
                entries.push(entry);
            }
            Failure::BlankValue { field } => {
                entries.push(at(
                    Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None),
                    field,
                ));
            }
            Failure::ValueInUse { field, orders, .. } => {
                entries.extend(orders.iter().map(|order| {
                    let mut entry = at(Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None), field);
                    entry.path = relative(order);
                    entry
                }));
            }
            Failure::UnconfirmedReusableValue { field, proposed } => {
                let mut entry = at(Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None), field);
                entry.after = c_text(proposed.as_str());
                entries.push(entry);
            }
            Failure::InvalidFactValue { fact, value } => {
                let mut entry = at(
                    Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None),
                    fact.code(),
                );
                entry.after = c_text(value.as_str());
                entries.push(entry);
            }
            Failure::PlanEvidenceUnconfirmed {
                outcome: EvidenceOutcome::Changed(stale),
            } => {
                entries.extend(stale.changed.iter().map(|changed| {
                    let mut entry =
                        Self::coded(SPIELORDERS_FAMILY_FAILURE, code, Some(&changed.name));
                    entry.after = c_text(match changed.change {
                        FactChange::Differs => "differs",
                        FactChange::Vanished => "vanished",
                        FactChange::Appeared => "appeared",
                        FactChange::Unobserved => "unobserved",
                    });
                    entry
                }));
            }
            Failure::PackageInvalid(diagnosis) | Failure::OrderInvalid(diagnosis) => {
                entries.push(Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None));
                entries.extend(
                    diagnosis
                        .diagnostic_groups
                        .iter()
                        .flat_map(|group| &group.diagnostics)
                        .map(Self::finding),
                );
            }
            Failure::VendorReferenced { orders, .. } => {
                entries.extend(orders.iter().map(|order| {
                    let mut entry = Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None);
                    entry.path = relative(order);
                    entry
                }));
            }
            Failure::DamagedOrder {
                paths, diagnostics, ..
            } => {
                let mut entry = Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None);
                entry.path = relative(&paths.directory);
                entries.push(entry);
                entries.extend(diagnostics.iter().map(Self::finding));
            }
            Failure::DamagedPackage(package) | Failure::PackageMetadataUnsupported(package) => {
                let mut entry = Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None);
                entry.path = relative(&package.context.root);
                entries.push(entry);
                entries.extend(
                    package
                        .diagnostic_groups
                        .iter()
                        .flat_map(|group| &group.diagnostics)
                        .map(Self::finding),
                );
            }
            Failure::CommitFailedCleanupIncomplete {
                failure: stopped,
                cleanup_failures,
            } => {
                let mut entry = Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None);
                entry.row = row;
                entry.path = relative(&stopped.location);
                entries.push(entry);
                entries.extend(cleanup_failures.iter().map(|left| {
                    let mut entry =
                        Self::coded(SPIELORDERS_FAMILY_FAILURE, code, Some("left behind"));
                    entry.row = row;
                    entry.path = relative(&left.location);
                    entry
                }));
            }
            Failure::IndexingDisabled(provider)
            | Failure::VolumeNotIndexed(provider)
            | Failure::AccessDenied(provider)
            | Failure::QueryRefused(provider)
            | Failure::Unsupported(provider) => {
                entries.push(Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None));
                entries.extend(
                    provider
                        .diagnostics
                        .iter()
                        .filter_map(|diagnostic| diagnostic.detail.as_deref())
                        .map(|said| Self::coded(SPIELORDERS_FAMILY_FAILURE, code, Some(said))),
                );
            }
            _ => {
                let (field, location) = subject_of(failure);
                let mut entry = Self::coded(SPIELORDERS_FAMILY_FAILURE, code, None);
                entry.row = row;
                if let Some(field) = field {
                    entry.field = subject.field(field, row >= 0);
                }
                if let Some(path) = location {
                    entry.path = path;
                }
                entries.push(entry);
            }
        }
    }
}

/// A list of failures, warnings, and findings. Read it with the
/// `spielorders_entries_*` accessors, each of which reads a NULL Entries as empty.
#[repr(C)]
pub struct SpielordersEntries {
    data: Box<SpielordersEntriesData>,
}

struct SpielordersEntriesData {
    entries: Vec<Entry>,
}

impl SpielordersEntries {
    /// The handle for `entries`, or `None` when there are none.
    pub(crate) fn of(entries: Vec<Entry>) -> Option<Box<Self>> {
        (!entries.is_empty()).then(|| {
            Box::new(Self {
                data: Box::new(SpielordersEntriesData { entries }),
            })
        })
    }

    pub(crate) fn clone_of(entries: &[Entry]) -> Option<Box<Self>> {
        Self::of(entries.to_vec())
    }

    fn entry(entries: Option<&Self>, index: i64) -> Option<&Entry> {
        let entries = &entries?.data.entries;
        entries.get(usize::try_from(index).ok()?)
    }
}

/// How many entries `entries` (or NULL) holds; 0 for NULL.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_count(entries: Option<&SpielordersEntries>) -> i64 {
    quietly(0, || {
        entries.map_or(0, |entries| {
            i64::try_from(entries.data.entries.len()).unwrap_or(i64::MAX)
        })
    })
}

/// The code of entry `index`: a `SPIELORDERS_FAILURE_*`, `SPIELORDERS_WARNING_*`, or
/// `SPIELORDERS_FINDING_*` constant. `entries` may be NULL. -1 when out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_code(
    entries: Option<&SpielordersEntries>,
    index: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        SpielordersEntries::entry(entries, index).map_or(SpielordersConstant::NONE, |e| e.code)
    })
}

/// The `SPIELORDERS_SEVERITY_*` of entry `index`. `entries` may be NULL. -1 when out of
/// range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_severity(
    entries: Option<&SpielordersEntries>,
    index: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        SpielordersEntries::entry(entries, index).map_or(SpielordersConstant::NONE, |e| e.severity)
    })
}

/// A message for entry `index` a caller may show. `entries` may be NULL. "" when out of
/// range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_message(
    entries: Option<&SpielordersEntries>,
    index: i64,
) -> Text<'_> {
    text_of(entries, index, |entry| &entry.message)
}

/// The field entry `index` is about: a `SPIELORDERS_*_FIELD_*` constant of the record or
/// row it locates, or -1. `entries` may be NULL.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_field(
    entries: Option<&SpielordersEntries>,
    index: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        SpielordersEntries::entry(entries, index).map_or(SpielordersConstant::NONE, |e| e.field)
    })
}

/// Why entry `index` rejected its field: a `SPIELORDERS_REASON_*` constant, or -1.
/// `entries` may be NULL.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_reason(
    entries: Option<&SpielordersEntries>,
    index: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        SpielordersEntries::entry(entries, index).map_or(SpielordersConstant::NONE, |e| e.reason)
    })
}

/// The zero-based row entry `index` is about, or -1 for the record's own facts.
/// `entries` may be NULL.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_row(entries: Option<&SpielordersEntries>, index: i64) -> i64 {
    quietly(-1, || {
        SpielordersEntries::entry(entries, index).map_or(-1, |e| e.row)
    })
}

/// The path entry `index` is about: Package-relative, or as named where it has no
/// Package-relative spelling; or "". `entries` may be NULL.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_path(
    entries: Option<&SpielordersEntries>,
    index: i64,
) -> Text<'_> {
    text_of(entries, index, |entry| &entry.path)
}

/// What a fact was, for a changed fact; or "". `entries` may be NULL.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_before(
    entries: Option<&SpielordersEntries>,
    index: i64,
) -> Text<'_> {
    text_of(entries, index, |entry| &entry.before)
}

/// What a fact is now, for a changed fact; a finding's evidence; a rejected value; or "".
/// `entries` may be NULL.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_after(
    entries: Option<&SpielordersEntries>,
    index: i64,
) -> Text<'_> {
    text_of(entries, index, |entry| &entry.after)
}

/// Frees `entries`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_entries_free(entries: Option<Box<SpielordersEntries>>) {
    quietly((), || drop(entries));
}

fn text_of<'a>(
    entries: Option<&'a SpielordersEntries>,
    index: i64,
    text: impl FnOnce(&'a Entry) -> &'a CString,
) -> Text<'a> {
    quietly(Text::new(EMPTY), || {
        SpielordersEntries::entry(entries, index)
            .map_or(Text::new(EMPTY), |entry| Text::new(text(entry)))
    })
}
