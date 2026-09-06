//! Package-relative locations, the stable diagnostic envelope, and exact
//! just-in-time reads of conventional Package and Order README documents.
//!
//! The README reads came first and named the module. What has accumulated beside
//! them is everything that describes *where* something is and *what is wrong with
//! it*: `Location` and the grammar that decides which paths this application can
//! express, and the diagnostic codes, severities, scopes, and typed evidence that
//! every reader reports through.

pub use crate::operations::{
    FailedPath, FileSystemOperation, OperationEvidence, OperationFailure, OperationFailureCode,
    PackageIndexReplacementPhase,
};
use crate::{
    digest::ContentDigest,
    metadata::{OrderId, VendorId},
    ports::{FileSystem, FileSystemEntryKind},
};
use std::{
    cmp::Ordering,
    ffi::OsStr,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationError {
    AbsolutePathRequired,
    InvalidPackageRelativePath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    absolute_path: PathBuf,
    package_relative_path: Option<String>,
}

impl Ord for Location {
    fn cmp(&self, other: &Self) -> Ordering {
        match (
            self.package_relative_path.as_deref(),
            other.package_relative_path.as_deref(),
        ) {
            (Some(left), Some(right)) => left
                .cmp(right)
                .then_with(|| self.absolute_path.cmp(&other.absolute_path)),
            (None, None) => self.absolute_path.cmp(&other.absolute_path),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Location {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Location {
    pub fn absolute(absolute_path: impl Into<PathBuf>) -> Result<Self, LocationError> {
        Self::new(absolute_path.into(), None)
    }

    pub fn within_package(
        absolute_path: impl Into<PathBuf>,
        package_relative_path: impl Into<String>,
    ) -> Result<Self, LocationError> {
        Self::new(absolute_path.into(), Some(package_relative_path.into()))
    }

    fn new(
        absolute_path: PathBuf,
        package_relative_path: Option<String>,
    ) -> Result<Self, LocationError> {
        if !absolute_path.is_absolute() {
            return Err(LocationError::AbsolutePathRequired);
        }
        if package_relative_path
            .as_deref()
            .is_some_and(|path| !is_normalized_package_relative(path))
        {
            return Err(LocationError::InvalidPackageRelativePath);
        }
        Ok(Self {
            absolute_path,
            package_relative_path,
        })
    }

    pub fn absolute_path(&self) -> &Path {
        &self.absolute_path
    }

    pub fn package_relative_path(&self) -> Option<&str> {
        self.package_relative_path.as_deref()
    }
}

fn is_normalized_package_relative(path: &str) -> bool {
    path == "."
        || (!path.is_empty()
            && !path.starts_with('/')
            && !matches!(path.as_bytes(), [drive, b':', ..] if drive.is_ascii_alphabetic())
            && !path.contains('\\')
            && path
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != ".."))
}

/// The normalized Package-relative spelling of `path` within `root`, when this
/// application has one for it.
///
/// `None` when `path` is not inside `root`, when one of the components between
/// them is not Unicode, or when the joined text is not a normalized
/// Package-relative path — the three ways a real filesystem entry can exist
/// inside a Package and still have no name a `Location` will carry.
///
/// This is the only place that answers the question. The code that must judge a
/// name a user chose and the code that assembles a path out of this crate's own
/// literal managed names ask it here rather than each carrying its own copy of
/// the grammar, because a copy that drifts lets one admit what the other refuses.
pub(crate) fn package_relative_path_text(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut text = relative
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?
        .join("/");
    if text.is_empty() {
        text.push('.');
    }
    is_normalized_package_relative(&text).then_some(text)
}

/// One path inside a Package, in the best spelling the Package has for it.
///
/// **The one place a capability turns a path it assembled into a reportable location.**
/// A response owes the Package-relative spelling for every path it names inside a Package,
/// and a caller deriving that spelling would be a second place deciding what a Package's
/// layout is. The absolute fallback is the honest answer for a path this application
/// cannot express relatively — it names the artifact rather than dropping it.
pub(crate) fn located_in_package(root: &Path, path: PathBuf) -> Location {
    package_relative_path_text(root, &path)
        .and_then(|relative| Location::within_package(path.clone(), relative).ok())
        .unwrap_or_else(|| {
            Location::absolute(path).expect("a managed path under a resolved Root is absolute")
        })
}

/// One location together with the Root a response names it under.
///
/// **Both facts or neither.** A capability that resolved a scope before refusing knows
/// which Root it resolved and how that Root spells the path it is refusing about, and a
/// response owes both — in a refusal exactly as in a success. Carrying them as one value
/// is what stops a refusal from reporting one and dropping the other.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootedLocation {
    root: PathBuf,
    location: Location,
}

impl RootedLocation {
    /// The SpielOrders Root this location is named under.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The location itself, in the best spelling that Root has for it.
    pub fn location(&self) -> &Location {
        &self.location
    }
}

/// One path a capability resolved inside `root`, ready to be reported with it.
pub(crate) fn rooted_at(root: &Path, path: PathBuf) -> RootedLocation {
    RootedLocation {
        root: root.to_path_buf(),
        location: located_in_package(root, path),
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReadmeKind {
    PackageReadme,
    OrderReadme,
}

impl ReadmeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PackageReadme => "package_readme",
            Self::OrderReadme => "order_readme",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReadmeReference {
    kind: ReadmeKind,
    location: Location,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadmeReferenceError {
    PackageRelativeLocationRequired,
    NotConventionalReadme,
    LocationMismatch,
    DocumentKindMismatch,
}

impl ReadmeReference {
    pub fn new(kind: ReadmeKind, location: Location) -> Result<Self, ReadmeReferenceError> {
        let package_relative_path = location
            .package_relative_path()
            .ok_or(ReadmeReferenceError::PackageRelativeLocationRequired)?;
        if location.absolute_path().file_name() != Some(OsStr::new("README.md"))
            || Path::new(package_relative_path).file_name() != Some(OsStr::new("README.md"))
        {
            return Err(ReadmeReferenceError::NotConventionalReadme);
        }
        if !location
            .absolute_path()
            .ends_with(Path::new(package_relative_path))
        {
            return Err(ReadmeReferenceError::LocationMismatch);
        }
        if matches!(kind, ReadmeKind::PackageReadme) != (package_relative_path == "README.md") {
            return Err(ReadmeReferenceError::DocumentKindMismatch);
        }
        Ok(Self { kind, location })
    }

    pub fn kind(&self) -> ReadmeKind {
        self.kind
    }

    pub fn location(&self) -> &Location {
        &self.location
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCode {
    ConfigurationPathInvalid,
    ConfigurationPathsOverlap,
    ConfiguredOrdersDirectoryNameUnusable,
    ConfiguredOrdersDirectoryUnavailable,
    ManagedArtifactInvalidEncoding,
    ManagedArtifactInvalidShape,
    ManagedArtifactInvalidSyntax,
    ManagedArtifactMissing,
    ManagedArtifactUnreadable,
    ManagedArtifactWrongKind,
    ManagedArtifactWrongVersion,
    OrderIdDuplicate,
    OrderPackageLocatorMismatch,
    OrderSemanticInvalid,
    OrdersDirectoryEntryDisallowed,
    OrdersDirectoryEntryNameUnusable,
    ReadmeUnavailable,
    ReservedContentUnexpected,
    StagingArtifactPresent,
    VendorIdDuplicate,
    VendorNameDuplicate,
    VendorReferenceUnresolved,
}

impl DiagnosticCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManagedArtifactMissing => "managed_artifact_missing",
            Self::ManagedArtifactWrongKind => "managed_artifact_wrong_kind",
            Self::ManagedArtifactUnreadable => "managed_artifact_unreadable",
            Self::ManagedArtifactInvalidEncoding => "managed_artifact_invalid_encoding",
            Self::ManagedArtifactInvalidSyntax => "managed_artifact_invalid_syntax",
            Self::ManagedArtifactInvalidShape => "managed_artifact_invalid_shape",
            Self::ManagedArtifactWrongVersion => "managed_artifact_wrong_version",
            Self::ConfigurationPathInvalid => "configuration_path_invalid",
            Self::ConfigurationPathsOverlap => "configuration_paths_overlap",
            Self::ConfiguredOrdersDirectoryUnavailable => "configured_orders_directory_unavailable",
            Self::ConfiguredOrdersDirectoryNameUnusable => {
                "configured_orders_directory_name_unusable"
            }
            Self::OrderSemanticInvalid => "order_semantic_invalid",
            Self::OrderPackageLocatorMismatch => "order_package_locator_mismatch",
            Self::OrderIdDuplicate => "order_id_duplicate",
            Self::VendorIdDuplicate => "vendor_id_duplicate",
            Self::VendorNameDuplicate => "vendor_name_duplicate",
            Self::VendorReferenceUnresolved => "vendor_reference_unresolved",
            Self::OrdersDirectoryEntryDisallowed => "orders_directory_entry_disallowed",
            Self::OrdersDirectoryEntryNameUnusable => "orders_directory_entry_name_unusable",
            Self::ReadmeUnavailable => "readme_unavailable",
            Self::ReservedContentUnexpected => "reserved_content_unexpected",
            Self::StagingArtifactPresent => "staging_artifact_present",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EvidenceScope {
    Package,
    Order,
    Document,
    Operation,
}

impl EvidenceScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::Order => "order",
            Self::Document => "document",
            Self::Operation => "operation",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReadmeUnavailableReason {
    WrongKind,
    Unreadable,
    InvalidEncoding,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ManagedArtifactKind {
    Config,
    Version,
    Vendors,
    Order,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConfigurationPathKind {
    OrdersPath,
    RemovedPath,
}

impl ConfigurationPathKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OrdersPath => "orders_path",
            Self::RemovedPath => "removed_path",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConfigurationPathReason {
    Absolute,
    CurrentDirectory,
    ParentTraversal,
    EmptyComponent,
    PlatformSeparator,
    ReservedMetadata,
    NotDirectChild,
    NotOrdinary,
}

impl ConfigurationPathReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Absolute => "absolute",
            Self::CurrentDirectory => "current_directory",
            Self::ParentTraversal => "parent_traversal",
            Self::EmptyComponent => "empty_component",
            Self::PlatformSeparator => "platform_separator",
            Self::ReservedMetadata => "reserved_metadata",
            Self::NotDirectChild => "not_direct_child",
            Self::NotOrdinary => "not_ordinary",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConfiguredOrdersDirectoryReason {
    NotDirectory,
    OutsidePackage,
    ReservedMetadata,
    /// The configured path resolves to a real name that has no normalized
    /// Package-relative spelling, so no location can carry it.
    NameUnusable,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OrderSemanticReason {
    InvalidGregorianDate,
    /// A calculated Item Total or Order Total is not a finite number.
    ///
    /// **Its own reason rather than a stored field's**, because overflow of two finite
    /// stored numbers is the only route to one: a non-finite Quantity or Unit Price never
    /// loads at all, so nothing stored is at fault and the `field` beside this reason is
    /// the calculated total instead. See `order::CalculatedTotal`.
    NonFiniteTotal,
}

impl OrderSemanticReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidGregorianDate => "invalid_gregorian_date",
            Self::NonFiniteTotal => "non_finite_total",
        }
    }
}

/// Why one Order's technical locator is damaged.
///
/// The four the integrity decision names, each repairable by recalculation: a member that
/// is not there, one that is not a locator, one that leads out of the active Orders
/// Directory, and one that leads to a Package this Order is not in.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OrderLocatorReason {
    Absent,
    Malformed,
    NotActiveDirectChild,
    DoesNotResolveToPackage,
}

impl OrderLocatorReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Malformed => "malformed",
            Self::NotActiveDirectChild => "not_active_direct_child",
            Self::DoesNotResolveToPackage => "does_not_resolve_to_package",
        }
    }
}

impl ConfiguredOrdersDirectoryReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotDirectory => "not_directory",
            Self::OutsidePackage => "outside_package",
            Self::ReservedMetadata => "reserved_metadata",
            Self::NameUnusable => "name_unusable",
        }
    }
}

impl ManagedArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Version => "version",
            Self::Vendors => "vendors",
            Self::Order => "order",
        }
    }
}

impl ReadmeUnavailableReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WrongKind => "wrong_kind",
            Self::Unreadable => "unreadable",
            Self::InvalidEncoding => "invalid_encoding",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEvidence {
    ManagedArtifact {
        artifact: ManagedArtifactKind,
        observed_kind: Option<FileSystemEntryKind>,
        operation: Option<FileSystemOperation>,
        field: Option<String>,
        observed_version: Option<u64>,
        expected_version: Option<u64>,
    },
    ConfigurationPathInvalid {
        path: ConfigurationPathKind,
        configured_value: String,
        reason: ConfigurationPathReason,
    },
    ConfigurationPathsOverlap {
        orders_path: String,
        removed_path: String,
    },
    ConfiguredOrdersDirectoryUnavailable {
        configured_value: String,
        reason: ConfiguredOrdersDirectoryReason,
    },
    /// The configured Orders path resolves, through a symbolic link, to a real name
    /// with no normalized Package-relative spelling, so nothing that describes the
    /// Package could refer to the directory it names. The configured value is
    /// carried because it is the text the user can act on; the resolved name is the
    /// one that has no spelling to report.
    ConfiguredOrdersDirectoryNameUnusable {
        configured_value: String,
    },
    OrdersDirectoryEntryDisallowed {
        observed_kind: FileSystemEntryKind,
    },
    /// One direct child of the Orders Directory whose filesystem name has no
    /// normalized Package-relative spelling. Its `primary_location` therefore
    /// carries an absolute path and no Package-relative one: the entry is inside
    /// the Package, and the missing spelling is the condition being reported.
    OrdersDirectoryEntryNameUnusable {
        observed_kind: FileSystemEntryKind,
    },
    ReservedContentUnexpected {
        observed_kind: FileSystemEntryKind,
    },
    /// An abandoned staging candidate sits beside a managed destination.
    ///
    /// `candidate_digest` identifies *which* candidate, because promotion writes that
    /// candidate's bytes: the path and the destination's validity are the same for
    /// every candidate that name has ever held, so neither can tell one from another.
    /// A digest rather than the bytes — a diagnostic is a report, and arbitrary user
    /// content does not belong in one.
    ///
    /// It is `None` when the entry has no readable content to identify: a directory or
    /// a link at the staging name, or a file that cannot be read. Nothing distinguishes
    /// two such candidates, and reporting no evidence is how that is said.
    StagingArtifactPresent {
        destination_valid: Option<bool>,
        candidate_digest: Option<ContentDigest>,
    },
    VendorIdDuplicate {
        vendor_id: VendorId,
    },
    VendorNameDuplicate {
        vendor_ids: Vec<VendorId>,
        observed_names: Vec<String>,
    },
    VendorReferenceUnresolved {
        order_id: OrderId,
        vendor_id: VendorId,
    },
    OrderSemanticInvalid {
        order_id: OrderId,
        field: String,
        reason: OrderSemanticReason,
    },
    OrderPackageLocatorMismatch {
        order_id: OrderId,
        /// The locator as the record stores it, and `None` where the record stores no
        /// text to quote back — an absent member, or one that is not text. The `reason`
        /// is what says which.
        package_root: Option<String>,
        reason: OrderLocatorReason,
    },
    OrderIdDuplicate {
        order_id: OrderId,
        conflict_locations: Vec<Location>,
    },
    ReadmeUnavailable {
        document_kind: ReadmeKind,
        reason: ReadmeUnavailableReason,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    code: DiagnosticCode,
    severity: Severity,
    scope: EvidenceScope,
    primary_location: Location,
    related_locations: Vec<Location>,
    evidence: DiagnosticEvidence,
}

impl Ord for Diagnostic {
    fn cmp(&self, other: &Self) -> Ordering {
        self.severity
            .cmp(&other.severity)
            .then_with(|| self.code.cmp(&other.code))
            .then_with(|| self.primary_location.cmp(&other.primary_location))
            .then_with(|| diagnostic_field(&self.evidence).cmp(&diagnostic_field(&other.evidence)))
            .then_with(|| self.evidence.cmp(&other.evidence))
            .then_with(|| self.related_locations.cmp(&other.related_locations))
            .then_with(|| self.scope.cmp(&other.scope))
    }
}

impl PartialOrd for Diagnostic {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn diagnostic_field(evidence: &DiagnosticEvidence) -> Option<&str> {
    match evidence {
        DiagnosticEvidence::ManagedArtifact { field, .. } => field.as_deref(),
        DiagnosticEvidence::OrderSemanticInvalid { field, .. } => Some(field),
        _ => None,
    }
}

impl Diagnostic {
    pub(crate) fn new(
        code: DiagnosticCode,
        severity: Severity,
        scope: EvidenceScope,
        primary_location: Location,
        related_locations: Vec<Location>,
        evidence: DiagnosticEvidence,
    ) -> Self {
        Self {
            code,
            severity,
            scope,
            primary_location,
            related_locations,
            evidence,
        }
    }

    pub(crate) fn managed_artifact(
        code: DiagnosticCode,
        scope: EvidenceScope,
        location: Location,
        artifact: ManagedArtifactKind,
        observed_kind: Option<FileSystemEntryKind>,
    ) -> Self {
        Self::managed_artifact_with_details(
            code,
            scope,
            location,
            artifact,
            observed_kind,
            None,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn managed_artifact_with_details(
        code: DiagnosticCode,
        scope: EvidenceScope,
        location: Location,
        artifact: ManagedArtifactKind,
        observed_kind: Option<FileSystemEntryKind>,
        field: Option<String>,
        observed_version: Option<u64>,
        expected_version: Option<u64>,
    ) -> Self {
        Self {
            code,
            severity: Severity::Error,
            scope,
            primary_location: location,
            related_locations: Vec::new(),
            evidence: DiagnosticEvidence::ManagedArtifact {
                artifact,
                observed_kind,
                operation: (code == DiagnosticCode::ManagedArtifactUnreadable)
                    .then_some(FileSystemOperation::Read),
                field,
                observed_version,
                expected_version,
            },
        }
    }

    fn unavailable(reference: &ReadmeReference, reason: ReadmeUnavailableReason) -> Self {
        Self {
            code: DiagnosticCode::ReadmeUnavailable,
            severity: Severity::Warning,
            scope: EvidenceScope::Document,
            primary_location: reference.location.clone(),
            related_locations: Vec::new(),
            evidence: DiagnosticEvidence::ReadmeUnavailable {
                document_kind: reference.kind,
                reason,
            },
        }
    }

    pub fn code(&self) -> DiagnosticCode {
        self.code
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn scope(&self) -> EvidenceScope {
        self.scope
    }

    pub fn primary_location(&self) -> &Location {
        &self.primary_location
    }

    pub fn related_locations(&self) -> &[Location] {
        &self.related_locations
    }

    pub fn evidence(&self) -> &DiagnosticEvidence {
        &self.evidence
    }
}

/// Shared stable diagnostic envelope. The historical name remains available for
/// exact README callers introduced by M2 Ticket 001.
/// Backward-compatible Ticket 001 name for README callers.
pub type ReadmeDiagnostic = Diagnostic;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadmeRead {
    reference: ReadmeReference,
    exists: bool,
    text: Option<String>,
    diagnostics: Vec<ReadmeDiagnostic>,
}

impl ReadmeRead {
    pub fn reference(&self) -> &ReadmeReference {
        &self.reference
    }

    pub fn exists(&self) -> bool {
        self.exists
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub fn diagnostics(&self) -> &[ReadmeDiagnostic] {
        &self.diagnostics
    }
}

pub fn read_readme<F: FileSystem>(
    reference: &ReadmeReference,
    filesystem: &F,
) -> Result<ReadmeRead, OperationFailure> {
    let kind = filesystem
        .symlink_metadata(reference.location.absolute_path())
        .map_err(|_| OperationFailure::observe_kind(&reference.location))?;
    match kind {
        None => Ok(ReadmeRead {
            reference: reference.clone(),
            exists: false,
            text: Some(String::new()),
            diagnostics: Vec::new(),
        }),
        Some(FileSystemEntryKind::File) => {
            let bytes = match filesystem.read(reference.location.absolute_path()) {
                Ok(bytes) => bytes,
                Err(_) => {
                    return Ok(unavailable_read(
                        reference,
                        ReadmeUnavailableReason::Unreadable,
                    ));
                }
            };
            let text = match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => {
                    return Ok(unavailable_read(
                        reference,
                        ReadmeUnavailableReason::InvalidEncoding,
                    ));
                }
            };
            Ok(ReadmeRead {
                reference: reference.clone(),
                exists: true,
                text: Some(text),
                diagnostics: Vec::new(),
            })
        }
        Some(_) => Ok(unavailable_read(
            reference,
            ReadmeUnavailableReason::WrongKind,
        )),
    }
}

fn unavailable_read(reference: &ReadmeReference, reason: ReadmeUnavailableReason) -> ReadmeRead {
    ReadmeRead {
        reference: reference.clone(),
        exists: true,
        text: None,
        diagnostics: vec![ReadmeDiagnostic::unavailable(reference, reason)],
    }
}
