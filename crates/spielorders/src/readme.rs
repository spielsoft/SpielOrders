//! Package-relative locations, the stable diagnostics, and exact just-in-time reads of
//! conventional Package and Order README documents.

use crate::operations::FailedOperation;
use crate::operations::FileSystemOperation;
use crate::{
    digest::ContentDigest,
    metadata::{OrderId, VendorId},
    ports::{FileSystem, FileSystemEntryKind},
};
use serde::Serialize;
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
#[non_exhaustive]
pub struct Location {
    pub absolute_path: PathBuf,
    /// The Package's own spelling, where the path is inside the Package and has one.
    pub package_relative_path: Option<String>,
}

enum RelativeSpelling {
    Managed(String),
    Native(String),
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
    pub(crate) fn absolute(absolute_path: impl Into<PathBuf>) -> Result<Self, LocationError> {
        Self::new(absolute_path.into(), None)
    }

    pub(crate) fn within_package(
        absolute_path: impl Into<PathBuf>,
        package_relative_path: impl Into<String>,
    ) -> Result<Self, LocationError> {
        Self::new(
            absolute_path.into(),
            Some(RelativeSpelling::Managed(package_relative_path.into())),
        )
    }

    /// An OS-reported user file, using platform components rather than managed-path grammar.
    pub(crate) fn supporting_material(root: &Path, absolute_path: PathBuf) -> Option<Self> {
        let text = normal_components_text(absolute_path.strip_prefix(root).ok()?)?;
        Self::new(absolute_path, Some(RelativeSpelling::Native(text))).ok()
    }

    fn new(
        absolute_path: PathBuf,
        package_relative_path: Option<RelativeSpelling>,
    ) -> Result<Self, LocationError> {
        if !absolute_path.is_absolute() {
            return Err(LocationError::AbsolutePathRequired);
        }
        let package_relative_path = package_relative_path
            .map(|relative| {
                let (valid, text) = match relative {
                    RelativeSpelling::Managed(text) => {
                        (is_normalized_package_relative(&text), text)
                    }
                    RelativeSpelling::Native(text) => (is_normalized_native_relative(&text), text),
                };
                valid
                    .then_some(text)
                    .ok_or(LocationError::InvalidPackageRelativePath)
            })
            .transpose()?;
        Ok(Self {
            absolute_path,
            package_relative_path,
        })
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

fn is_normalized_native_relative(path: &str) -> bool {
    !path.is_empty() && normal_components_text(Path::new(path)).as_deref() == Some(path)
}

fn normal_components_text(path: &Path) -> Option<String> {
    path.components()
        .map(|component| match component {
            std::path::Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(|components| components.join("/"))
}

/// The normalized managed-path spelling of `path` relative to `root`.
///
/// `None` when `path` is not inside `root`, when one of the components between
/// them is not Unicode, or when the joined text is not a normalized
/// Package-relative path — the three ways a real filesystem entry can exist
/// inside a Package and still have no name a `Location` will carry.
///
/// Component spelling is shared with native Supporting Material locations; managed and
/// native validation remain distinct at the checked `Location` constructor.
pub(crate) fn package_relative_path_text(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut text = normal_components_text(relative)?;
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
#[non_exhaustive]
pub struct RootedLocation {
    /// The SpielOrders Root this location is named under.
    pub root: PathBuf,
    /// The location itself, in the best spelling that Root has for it.
    pub location: Location,
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

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct ReadmeReference {
    pub kind: ReadmeKind,
    pub location: Location,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadmeReferenceError {
    PackageRelativeLocationRequired,
    NotConventionalReadme,
    LocationMismatch,
    DocumentKindMismatch,
}

impl ReadmeReference {
    pub(crate) fn new(kind: ReadmeKind, location: Location) -> Result<Self, ReadmeReferenceError> {
        let package_relative_path = location
            .package_relative_path
            .as_deref()
            .ok_or(ReadmeReferenceError::PackageRelativeLocationRequired)?;
        if location.absolute_path.file_name() != Some(OsStr::new("README.md"))
            || Path::new(package_relative_path).file_name() != Some(OsStr::new("README.md"))
        {
            return Err(ReadmeReferenceError::NotConventionalReadme);
        }
        if !location
            .absolute_path
            .ends_with(Path::new(&package_relative_path))
        {
            return Err(ReadmeReferenceError::LocationMismatch);
        }
        if matches!(kind, ReadmeKind::PackageReadme) != (package_relative_path == "README.md") {
            return Err(ReadmeReferenceError::DocumentKindMismatch);
        }
        Ok(Self { kind, location })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
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

impl crate::code::Coded for DiagnosticCode {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EvidenceScope {
    Package,
    Order,
    Document,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReadmeUnavailableReason {
    WrongKind,
    Unreadable,
    AccessDenied,
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConfiguredOrdersDirectoryReason {
    NotDirectory,
    OutsidePackage,
    ReservedMetadata,
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
        // Which configured key, named apart from the filesystem paths beside it.
        configuration_path: ConfigurationPathKind,
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
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub scope: EvidenceScope,
    pub primary_location: Location,
    pub related_locations: Vec<Location>,
    pub evidence: DiagnosticEvidence,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadmeRead {
    pub reference: ReadmeReference,
    pub exists: bool,
    pub text: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Reads one conventional README: absent is an empty text, and an unreadable or non-text
/// document is a finding beside its read rather than a failure.
pub(crate) fn read_document<F: FileSystem>(
    reference: &ReadmeReference,
    filesystem: &F,
) -> Result<ReadmeRead, FailedOperation> {
    let kind = filesystem
        .symlink_metadata(&reference.location.absolute_path)
        .map_err(|error| {
            FailedOperation::filesystem(reference.location.clone(), &error, filesystem)
        })?;
    match kind {
        None => Ok(ReadmeRead {
            reference: reference.clone(),
            exists: false,
            text: Some(String::new()),
            diagnostics: Vec::new(),
        }),
        Some(FileSystemEntryKind::File) => {
            let bytes = match filesystem.read(&reference.location.absolute_path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Ok(unavailable_read(
                        reference,
                        if filesystem.is_access_denied(&error) {
                            ReadmeUnavailableReason::AccessDenied
                        } else {
                            ReadmeUnavailableReason::Unreadable
                        },
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
        diagnostics: vec![Diagnostic::unavailable(reference, reason)],
    }
}

// Decision sources: Define the stable M2 diagnostic vocabulary and evidence contract;
// Define the SpielOrders package topology.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locations_require_absolute_native_and_normalized_package_relative_paths() {
        let outside = Location::absolute("/outside/README.md")
            .expect("absolute outside location is representable");
        assert_eq!(
            outside.absolute_path,
            std::path::Path::new("/outside/README.md")
        );
        assert_eq!(outside.package_relative_path.as_deref(), None);

        let inside = Location::within_package(
            "/package/Orders/Optics/README.md",
            "Orders/Optics/README.md",
        )
        .expect("normalized established Package location is representable");
        assert_eq!(
            inside.package_relative_path.as_deref(),
            Some("Orders/Optics/README.md")
        );

        assert_eq!(
            Location::absolute("relative/README.md"),
            Err(LocationError::AbsolutePathRequired)
        );
        for invalid in [
            "",
            "/README.md",
            "C:/README.md",
            "C:README.md",
            "Orders\\README.md",
            "Orders//README.md",
            "Orders/./README.md",
            "Orders/../README.md",
        ] {
            assert_eq!(
                Location::within_package("/package/README.md", invalid),
                Err(LocationError::InvalidPackageRelativePath),
                "{invalid:?} must not become Package-relative evidence"
            );
        }
    }

    #[test]
    fn readme_references_require_exact_established_conventional_locations() {
        let unestablished = Location::absolute("/package/README.md")
            .expect("absolute locations may exist before Package membership is established");
        assert_eq!(
            ReadmeReference::new(ReadmeKind::PackageReadme, unestablished),
            Err(ReadmeReferenceError::PackageRelativeLocationRequired)
        );

        let ordinary_document = Location::within_package("/package/notes.md", "notes.md")
            .expect("ordinary document location is structurally valid");
        assert_eq!(
            ReadmeReference::new(ReadmeKind::PackageReadme, ordinary_document),
            Err(ReadmeReferenceError::NotConventionalReadme)
        );

        let mismatched =
            Location::within_package("/package/Orders/A/README.md", "Orders/B/README.md")
                .expect("both individual paths are normalized");
        assert_eq!(
            ReadmeReference::new(ReadmeKind::OrderReadme, mismatched),
            Err(ReadmeReferenceError::LocationMismatch)
        );

        let order_location =
            Location::within_package("/package/Orders/A/README.md", "Orders/A/README.md")
                .expect("Order README location is valid");
        assert_eq!(
            ReadmeReference::new(ReadmeKind::PackageReadme, order_location),
            Err(ReadmeReferenceError::DocumentKindMismatch)
        );
        let package_location = Location::within_package("/package/README.md", "README.md")
            .expect("Package README location is valid");
        assert_eq!(
            ReadmeReference::new(ReadmeKind::OrderReadme, package_location),
            Err(ReadmeReferenceError::DocumentKindMismatch)
        );
    }
}
