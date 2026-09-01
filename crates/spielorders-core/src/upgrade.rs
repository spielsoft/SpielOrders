//! Executing and resuming the registered consecutive Metadata Upgrade.
//!
//! This is the one place in the write side where partial durable progress is
//! intended rather than prevented. Valid artifacts upgrade atomically one at a time
//! and stay upgraded across an interruption; the Package marker advances only after
//! every artifact has, so an interrupted upgrade never looks complete. Resumption
//! reads current state rather than a stored progress marker, and so does the commit
//! for every artifact it is about to act on: a plan's classification is evidence about
//! a moment that has passed, never a licence to transform a record someone else already
//! finished or to pass over one that went back. An artifact the dry run already named
//! as one it cannot interpret is the exception and is not reopened — the run stops
//! there, exactly where the plan said it would.
//!
//! An upgrade is not a repair. Every artifact it reads or writes at a version this
//! repository publishes a schema for is held to that schema, and every step's output
//! must declare exactly the version that step was to reach. What it cannot interpret,
//! it pauses at.
//!
//! The running build knows exactly one durable metadata version, so its registered
//! chain is empty and an upgrade is always already complete. The orchestration is
//! nonetheless real, and a caller may supply bounded transformations to drive it;
//! doing so publishes no durable schema and makes no future version part of the
//! product contract.

use crate::{
    canonical::{PackageIndexRecord, PackageLoad, open_package},
    filesystem::{
        DestinationExpectation, ValidatedReplacementError, replace_validated_file_expecting,
    },
    metadata::{Config, OrderDocument, VendorsDocument, Version1, VersionDocument},
    mutation::MutationReport,
    operations::OperationFailure,
    package::PackageMetadata,
    paths::{PackagePathError, resolve_package_layout},
    ports::{FileSystem, FileSystemEntryKind},
};
use std::path::{Path, PathBuf};

/// The newest durable metadata version this build understands.
pub const NEWEST_METADATA_VERSION: u64 = 1;

/// Which managed artifact a transformation is being asked to convert.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeArtifact {
    OrderRecord,
    VendorAggregate,
}

impl UpgradeArtifact {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OrderRecord => "order_record",
            Self::VendorAggregate => "vendor_aggregate",
        }
    }
}

/// Why a registered transformation declined an artifact it was given. A refusal
/// pauses the run; it never becomes a salvage or a guess.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransformationRefused;

/// One registered transformation from a version to the single version after it.
/// Only consecutive steps are expressible, so an arbitrary target, a skipped
/// version, and a downgrade have no representation to begin with.
pub trait MetadataTransformation {
    /// The version this step reads. It always produces `source_version() + 1`.
    fn source_version(&self) -> u64;

    fn transform(
        &self,
        artifact: UpgradeArtifact,
        bytes: &[u8],
    ) -> Result<Vec<u8>, TransformationRefused>;
}

/// The consecutive transformations this build has registered.
#[derive(Default)]
pub struct TransformationRegistry<'a> {
    steps: Vec<&'a dyn MetadataTransformation>,
}

impl<'a> TransformationRegistry<'a> {
    /// The chain this build ships. There is one durable version, so there is nothing
    /// to transform and every Package is already at the newest version.
    pub fn registered() -> Self {
        Self::default()
    }

    pub fn with(mut self, step: &'a dyn MetadataTransformation) -> Self {
        self.steps.push(step);
        self
    }

    /// The consecutive chain from `source`, stopping as soon as no registered step
    /// reads the version reached. A gap therefore truncates the chain rather than
    /// being bridged.
    fn chain_from(&self, source: u64) -> Vec<&'a dyn MetadataTransformation> {
        let mut chain = Vec::new();
        let mut version = source;
        while let Some(step) = self
            .steps
            .iter()
            .find(|step| step.source_version() == version)
        {
            chain.push(*step);
            version += 1;
        }
        chain
    }

    fn newest_from(&self, source: u64) -> u64 {
        source + self.chain_from(source).len() as u64
    }

    /// A version two registered steps both claim to read, when there is one.
    ///
    /// Two steps reading the same version are two different answers to one question,
    /// and picking either would migrate with a step nobody chose while the other left
    /// no trace of having been registered. This asks of the whole registry rather than
    /// only of the chain a particular Package needs, because the registry is the
    /// build's declaration: an ambiguity in it is a defect whichever Package is opened,
    /// and diagnosing it only for the Packages that reach it would hide the defect
    /// until the wrong Package showed up.
    fn repeated_source_version(&self) -> Option<u64> {
        let mut declared = self
            .steps
            .iter()
            .map(|step| step.source_version())
            .collect::<Vec<_>>();
        declared.sort_unstable();
        declared
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeArtifactState {
    /// At the source version, so this artifact will be upgraded.
    Pending,
    /// Already at the target version, so a resumed upgrade skips it.
    AlreadyUpgraded,
    /// Neither the source nor the target version, or unreadable, so the upgrade
    /// cannot proceed past it.
    Blocked,
}

impl UpgradeArtifactState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::AlreadyUpgraded => "already_upgraded",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeArtifactPlan {
    artifact: UpgradeArtifact,
    path: PathBuf,
    state: UpgradeArtifactState,
}

impl UpgradeArtifactPlan {
    pub fn artifact(&self) -> UpgradeArtifact {
        self.artifact
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn state(&self) -> UpgradeArtifactState {
        self.state
    }
}

/// One durable write this upgrade will attempt.
///
/// The marker advance and the index rebuild are operations exactly like a record
/// replacement, and they are the two package-wide ones. A plan that listed only the
/// records would under-report its own effects, and a caller could not tell from it
/// that the run rewrites the Package marker at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataUpgradeOperationKind {
    ReplaceArtifact,
    AdvanceMarker,
    RefreshPackageIndex,
}

impl MetadataUpgradeOperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReplaceArtifact => "replace_artifact",
            Self::AdvanceMarker => "advance_marker",
            Self::RefreshPackageIndex => "refresh_package_index",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataUpgradeOperation {
    kind: MetadataUpgradeOperationKind,
    path: PathBuf,
}

impl MetadataUpgradeOperation {
    pub fn kind(&self) -> MetadataUpgradeOperationKind {
        self.kind
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The complete deterministic upgrade this Package needs, together with the exact
/// consecutive chain that will carry it out.
///
/// The chain is part of the plan rather than a second argument to the commit because
/// everything else here was derived from it: the target version, the classification of
/// every artifact, and the version the marker will be advanced to. A commit driven by
/// a different chain would rewrite records with steps this plan never assessed and
/// then advance the marker to a version it never verified.
#[derive(Clone)]
pub struct MetadataUpgradePlan<'a> {
    root: PathBuf,
    marker_path: PathBuf,
    source_version: u64,
    target_version: u64,
    artifacts: Vec<UpgradeArtifactPlan>,
    operations: Vec<MetadataUpgradeOperation>,
    chain: Vec<&'a dyn MetadataTransformation>,
}

/// A transformation has no representation worth printing; the versions its steps read
/// are the part a reader needs, and they are what makes one chain distinguishable from
/// another.
impl std::fmt::Debug for MetadataUpgradePlan<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MetadataUpgradePlan")
            .field("root", &self.root)
            .field("marker_path", &self.marker_path)
            .field("source_version", &self.source_version)
            .field("target_version", &self.target_version)
            .field("artifacts", &self.artifacts)
            .field("operations", &self.operations)
            .field(
                "chain",
                &self
                    .chain
                    .iter()
                    .map(|step| step.source_version())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl MetadataUpgradePlan<'_> {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source_version(&self) -> u64 {
        self.source_version
    }

    /// The newest version the running build can reach by consecutive registered
    /// steps. It is never chosen by a caller.
    pub fn target_version(&self) -> u64 {
        self.target_version
    }

    /// Every artifact in the exact order the upgrade will visit it: each Order
    /// Directory in deterministic order, then the Vendor aggregate.
    pub fn artifacts(&self) -> &[UpgradeArtifactPlan] {
        &self.artifacts
    }

    /// Every durable write this run will attempt, in the order it will attempt them:
    /// one replacement per outstanding artifact, then the marker advance, then the
    /// index rebuild.
    ///
    /// An artifact already at the target is not work, so it is not an operation. A run
    /// stops at the first artifact it cannot interpret, so the operations end there
    /// too — including the marker advance, which is exactly what a paused run must not
    /// reach.
    pub fn operations(&self) -> &[MetadataUpgradeOperation] {
        &self.operations
    }

    pub fn blockers(&self) -> Vec<&UpgradeArtifactPlan> {
        self.artifacts
            .iter()
            .filter(|artifact| artifact.state == UpgradeArtifactState::Blocked)
            .collect()
    }

    /// Whether anything remains to do.
    ///
    /// The Package marker settles it, and it can only settle it because it advances
    /// last: a marker at the target is the durable statement that every artifact got
    /// there first. While it is old the upgrade is unfinished however many artifacts
    /// are already done — an interrupted run leaves exactly that state, and the marker
    /// is what keeps ordinary use blocked, so advancing it is the outstanding work.
    ///
    /// An artifact this run could not interpret is reported by [`Self::blockers`] and
    /// does not make a Package whose marker is already current incomplete. There is no
    /// chain to run on it; a record this build cannot read at the version the Package
    /// already declares is a Damaged Order, which integrity repair answers.
    pub fn is_complete(&self) -> bool {
        self.source_version == self.target_version
    }
}

#[derive(Debug, PartialEq)]
pub enum MetadataUpgradeError<E> {
    /// No Package marker, so there is nothing to upgrade.
    UnmanagedPackage {
        path: PathBuf,
    },
    /// The marker is missing, unreadable, or does not declare a version. It is
    /// diagnosed rather than interpreted.
    UnreadableMarker {
        path: PathBuf,
    },
    /// The Package declares a version newer than this build knows. It is left
    /// untouched.
    NewerThanBuild {
        observed: u64,
        newest: u64,
    },
    /// The Package declares an older version this build has no complete consecutive
    /// chain for. It is left untouched rather than partially migrated.
    NoCompleteChain {
        observed: u64,
        reachable: u64,
    },
    /// Two registered steps declare the same source version, so the chain has two
    /// different answers for one step. Nothing is read and nothing is migrated: which
    /// transformations this build ships is not a question a Package can settle.
    IncoherentChain {
        version: u64,
    },
    /// The Package configuration that names the Orders Directory is missing,
    /// unreadable, or unusable. The upgrade refuses rather than assuming the default
    /// location: assuming wrong would enumerate the wrong directory, find nothing
    /// pending, and advance the marker past records that were never migrated.
    UnusableConfiguration {
        path: PathBuf,
    },
    FileSystem {
        path: PathBuf,
        error: E,
    },
}

pub fn plan_metadata_upgrade<'a, F: FileSystem>(
    root: &Path,
    registry: &TransformationRegistry<'a>,
    filesystem: &F,
) -> Result<MetadataUpgradePlan<'a>, MetadataUpgradeError<F::Error>> {
    if let Some(version) = registry.repeated_source_version() {
        return Err(MetadataUpgradeError::IncoherentChain { version });
    }
    let marker_path = root.join(".spielorders").join("version.json");
    match filesystem.symlink_metadata(&marker_path) {
        Ok(Some(FileSystemEntryKind::File)) => {}
        Ok(_) => {
            return Err(MetadataUpgradeError::UnmanagedPackage {
                path: root.to_path_buf(),
            });
        }
        Err(error) => {
            return Err(MetadataUpgradeError::FileSystem {
                path: marker_path,
                error,
            });
        }
    }
    let source_version = declared_version(&marker_path, "metadata_version", filesystem)?
        .ok_or_else(|| MetadataUpgradeError::UnreadableMarker {
            path: marker_path.clone(),
        })?;

    // The upgrade always targets exactly the newest version this build knows. It is
    // never chosen by a caller, never partial, and never downward.
    let target_version = registry.newest_from(source_version);
    if source_version > NEWEST_METADATA_VERSION {
        return Err(MetadataUpgradeError::NewerThanBuild {
            observed: source_version,
            newest: NEWEST_METADATA_VERSION,
        });
    }
    if target_version != NEWEST_METADATA_VERSION {
        return Err(MetadataUpgradeError::NoCompleteChain {
            observed: source_version,
            reachable: target_version,
        });
    }

    let mut artifacts = Vec::new();
    // Orders first, in deterministic directory order, then the Vendor aggregate.
    let orders_directory = configured_orders_directory(root, filesystem)?;
    for directory in order_directories(&orders_directory, filesystem)? {
        let path = directory.join(".spielorders").join("order.json");
        let state = artifact_state(
            UpgradeArtifact::OrderRecord,
            &path,
            source_version,
            target_version,
            filesystem,
        )?;
        artifacts.push(UpgradeArtifactPlan {
            artifact: UpgradeArtifact::OrderRecord,
            path,
            state,
        });
    }
    let vendors_path = root.join(".spielorders").join("vendors.json");
    let state = artifact_state(
        UpgradeArtifact::VendorAggregate,
        &vendors_path,
        source_version,
        target_version,
        filesystem,
    )?;
    artifacts.push(UpgradeArtifactPlan {
        artifact: UpgradeArtifact::VendorAggregate,
        path: vendors_path,
        state,
    });

    // A Package already at the target carries nothing anywhere, and the commit reports
    // it unchanged. Enumerating a marker advance and an index rebuild here would have
    // the plan advertise two writes that will never happen.
    let operations = if source_version == target_version {
        Vec::new()
    } else {
        planned_operations(root, &marker_path, &artifacts)
    };
    Ok(MetadataUpgradePlan {
        root: root.to_path_buf(),
        marker_path,
        source_version,
        target_version,
        artifacts,
        operations,
        chain: registry.chain_from(source_version),
    })
}

/// Enumerates the writes the run will attempt, in the order the commit performs them.
///
/// A blocked artifact stops the run where it stands, so nothing after it is planned —
/// neither the artifacts behind it nor the marker whose advance would declare them all
/// migrated.
fn planned_operations(
    root: &Path,
    marker_path: &Path,
    artifacts: &[UpgradeArtifactPlan],
) -> Vec<MetadataUpgradeOperation> {
    let mut operations = Vec::new();
    for artifact in artifacts {
        match artifact.state {
            UpgradeArtifactState::AlreadyUpgraded => continue,
            UpgradeArtifactState::Blocked => return operations,
            UpgradeArtifactState::Pending => operations.push(MetadataUpgradeOperation {
                kind: MetadataUpgradeOperationKind::ReplaceArtifact,
                path: artifact.path.clone(),
            }),
        }
    }
    operations.push(MetadataUpgradeOperation {
        kind: MetadataUpgradeOperationKind::AdvanceMarker,
        path: marker_path.to_path_buf(),
    });
    operations.push(MetadataUpgradeOperation {
        kind: MetadataUpgradeOperationKind::RefreshPackageIndex,
        path: root.join(".spielorders").join("index.json"),
    });
    operations
}

/// The Orders Directory this Package actually configured.
///
/// `orders_path` is configurable and persisted by initialization, and every other
/// capability resolves it through the layout. Assuming `Orders/` here would enumerate
/// the wrong directory on a configured Package: when no such directory exists the
/// upgrade fails for a reason that names the wrong path, and when an unrelated one
/// does exist the run finds nothing pending and advances the marker past every Order
/// record it never migrated. So this refuses rather than falling back.
fn configured_orders_directory<F: FileSystem>(
    root: &Path,
    filesystem: &F,
) -> Result<PathBuf, MetadataUpgradeError<F::Error>> {
    let config_path = root.join(".spielorders").join("config.toml");
    let bytes =
        filesystem
            .read(&config_path)
            .map_err(|error| MetadataUpgradeError::FileSystem {
                path: config_path.clone(),
                error,
            })?;
    let config = Config::from_toml_bytes(&bytes).map_err(|_| {
        MetadataUpgradeError::UnusableConfiguration {
            path: config_path.clone(),
        }
    })?;
    match resolve_package_layout(root, &config, filesystem) {
        Ok(layout) => Ok(layout.orders_directory().to_path_buf()),
        Err(PackagePathError::FileSystem(error)) => Err(MetadataUpgradeError::FileSystem {
            path: config_path,
            error,
        }),
        Err(_) => Err(MetadataUpgradeError::UnusableConfiguration { path: config_path }),
    }
}

fn order_directories<F: FileSystem>(
    orders: &Path,
    filesystem: &F,
) -> Result<Vec<PathBuf>, MetadataUpgradeError<F::Error>> {
    let orders = orders.to_path_buf();
    let mut entries = match filesystem.read_directory(&orders) {
        Ok(entries) => entries,
        Err(error) => {
            return Err(MetadataUpgradeError::FileSystem {
                path: orders,
                error,
            });
        }
    };
    entries.sort_by(|left, right| left.name().cmp(right.name()));
    Ok(entries
        .into_iter()
        .filter(|entry| {
            entry.kind() == FileSystemEntryKind::Directory
                && !entry.name().as_encoded_bytes().starts_with(b".")
        })
        .map(|entry| orders.join(entry.name()))
        .collect())
}

/// Classifies one artifact against the source and target versions. Anything that is
/// neither is blocked: this core never interprets an unrecognized version.
fn artifact_state<F: FileSystem>(
    artifact: UpgradeArtifact,
    path: &Path,
    source: u64,
    target: u64,
    filesystem: &F,
) -> Result<UpgradeArtifactState, MetadataUpgradeError<F::Error>> {
    let Ok(bytes) = filesystem.read(path) else {
        return Ok(UpgradeArtifactState::Blocked);
    };
    Ok(classify(artifact, &bytes, source, target))
}

/// The same judgment made from bytes already in hand, so planning and committing
/// cannot classify the same artifact by different rules.
///
/// The declared version alone is not enough. Where this repository publishes a schema
/// for the version an artifact declares, the artifact is held to it: an upgrade may
/// only transform a source artifact that is already valid under its declared schema,
/// and an artifact declaring the target version while breaking that version's rules is
/// not finished work to skip past.
fn classify(
    artifact: UpgradeArtifact,
    bytes: &[u8],
    source: u64,
    target: u64,
) -> UpgradeArtifactState {
    // An artifact this build cannot read as the version it declares is blocked whichever
    // of the two versions that is: an upgrade transforms and passes over valid records
    // only, and never interprets anything else.
    let unless_invalid = |version, state| {
        if artifact_is_valid_at(artifact, bytes, version).is_ok() {
            state
        } else {
            UpgradeArtifactState::Blocked
        }
    };
    match declared_version_in(bytes, "schema_version") {
        Some(version) if version == target => {
            unless_invalid(target, UpgradeArtifactState::AlreadyUpgraded)
        }
        Some(version) if version == source => unless_invalid(source, UpgradeArtifactState::Pending),
        _ => UpgradeArtifactState::Blocked,
    }
}

fn declared_version<F: FileSystem>(
    path: &Path,
    field: &str,
    filesystem: &F,
) -> Result<Option<u64>, MetadataUpgradeError<F::Error>> {
    let bytes = match filesystem.read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    Ok(declared_version_in(&bytes, field))
}

fn declared_version_in(bytes: &[u8], field: &str) -> Option<u64> {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|value| value.get(field).and_then(serde_json::Value::as_u64))
}

/// Whether these bytes really are this artifact at this version.
///
/// Version 1 is the one durable contract this repository publishes a schema for, so it
/// is the only version whose shape and semantics can be checked at all. For any other
/// version the declared number is the whole of what is knowable, and asserting more
/// would be inventing a schema this build does not have.
fn artifact_is_valid_at(
    artifact: UpgradeArtifact,
    bytes: &[u8],
    version: u64,
) -> Result<(), UpgradeArtifactFailure> {
    if version != NEWEST_METADATA_VERSION {
        return Ok(());
    }
    match artifact {
        UpgradeArtifact::OrderRecord => {
            let document = OrderDocument::from_json_bytes(bytes)
                .map_err(|_| UpgradeArtifactFailure::NotTheDeclaredSchema)?;
            // An Order's own semantics only. Whether its Vendor Reference resolves is a
            // Package-wide question, and during an upgrade the Vendor aggregate is
            // still at the source version — it is migrated last — so there is no
            // trustworthy Vendor set to ask. The Package Index rebuild that follows the
            // marker advance asks that question once, of the whole migrated Package.
            document
                .validate()
                .map(|_| ())
                .map_err(|_| UpgradeArtifactFailure::InvalidAtDeclaredVersion)
        }
        UpgradeArtifact::VendorAggregate => {
            let vendors = VendorsDocument::from_json_bytes(bytes)
                .map_err(|_| UpgradeArtifactFailure::NotTheDeclaredSchema)?;
            // Asked through the Package-wide validator rather than a local scan, so an
            // aggregate cannot be written on terms canonical validation would reject.
            // The empty Orders slice is the same call `canonical` makes when it decides
            // whether a stored `vendors.json` damages the Package: the aggregate answers
            // for itself — duplicate Vendor IDs and package-duplicate Vendor Names.
            //
            // This is the fourth occurrence of that shape. Hoisting it would mean
            // editing `canonical.rs` and `vendors.rs`, which is outside this ticket's
            // review boundary, so it is repeated here for the same recorded reason as
            // the third.
            let marker = VersionDocument {
                metadata_version: Version1,
            };
            PackageMetadata::new(&marker, &[], &vendors)
                .validate()
                .map(|_| ())
                .map_err(|_| UpgradeArtifactFailure::InvalidAtDeclaredVersion)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataUpgradeResult {
    source_version: u64,
    reached_version: u64,
    upgraded: Vec<PathBuf>,
    skipped: Vec<PathBuf>,
    marker_advanced: bool,
    index: Option<PackageIndexRecord>,
}

impl MetadataUpgradeResult {
    pub fn source_version(&self) -> u64 {
        self.source_version
    }

    /// The version the Package marker now declares.
    pub fn reached_version(&self) -> u64 {
        self.reached_version
    }

    /// Every artifact this run transformed, in the order it visited them.
    pub fn upgraded(&self) -> &[PathBuf] {
        &self.upgraded
    }

    /// Artifacts already at the target, which a resumed run leaves exactly as they
    /// are rather than transforming twice.
    pub fn skipped(&self) -> &[PathBuf] {
        &self.skipped
    }

    pub fn marker_advanced(&self) -> bool {
        self.marker_advanced
    }

    pub fn index(&self) -> Option<&PackageIndexRecord> {
        self.index.as_ref()
    }
}

/// Why some bytes are not the managed artifact they claim to be.
///
/// One judgement serves both directions. The artifacts a run reads are held to it, so
/// an upgrade only ever transforms or passes over an artifact already valid under the
/// version it declares; and what a step produced is held to it before anything is
/// staged, because an output this build will not write pauses the run for the user to
/// resolve rather than failing a write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeArtifactFailure {
    /// The bytes declare a version other than the one expected of them here. They are
    /// therefore not where the run needs them however well-formed they are, and
    /// carrying them on would let the marker advance over an artifact that never got
    /// to the target.
    WrongDeclaredVersion,
    /// This repository has a schema for the version they declare and they are not a
    /// record under it.
    NotTheDeclaredSchema,
    /// They are a record at the version they declare, but not one that version's
    /// semantics accept.
    InvalidAtDeclaredVersion,
}

impl UpgradeArtifactFailure {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WrongDeclaredVersion => "wrong_declared_version",
            Self::NotTheDeclaredSchema => "not_the_declared_schema",
            Self::InvalidAtDeclaredVersion => "invalid_at_declared_version",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// A Metadata Upgrade writes bytes a registered transformation produced, and this
/// pattern validates nothing further on them: those bytes were already judged, and an
/// output this build will not write paused the run before reaching a staging file. No
/// candidate validation failure is possible at this seam, which this states.
pub enum UpgradeCandidateFailure {}

#[derive(Debug, PartialEq)]
pub enum MetadataUpgradeRejection<E> {
    ArtifactReplacementFailed {
        path: PathBuf,
        error: ValidatedReplacementError<E, UpgradeCandidateFailure>,
    },
    MarkerAdvanceFailed {
        path: PathBuf,
        error: ValidatedReplacementError<E, UpgradeCandidateFailure>,
    },
}

// Matches `MetadataUpgradeRejection`: the typed replacement error these variants
// carry is not itself cloneable or totally comparable.
#[derive(Debug, PartialEq)]
pub enum MetadataUpgradeWarning<E> {
    /// An artifact this upgrade cannot interpret stopped the run at its deterministic
    /// position. Everything before it stayed upgraded and the marker did not advance;
    /// the user decides how to resolve it, and rerunning resumes.
    PausedAtBlockedArtifact {
        path: PathBuf,
    },
    /// A transformation refused an artifact it was given, which stops the run in the
    /// same way.
    PausedAtRefusedTransformation {
        path: PathBuf,
    },
    /// The chain produced something this build will not write at the target version,
    /// which stops the run in the same way. Nothing was staged and the artifact keeps
    /// its previous contents.
    PausedAtUnusableOutput {
        path: PathBuf,
        failure: UpgradeArtifactFailure,
    },
    /// An artifact could not be replaced after earlier artifacts had already been
    /// upgraded. Each replacement is atomic, so this one left prior bytes intact, but
    /// the run as a whole made durable progress and cannot be reported as a rejection.
    StoppedAtFailedArtifact {
        path: PathBuf,
        error: ValidatedReplacementError<E, UpgradeCandidateFailure>,
    },
    /// Every artifact reached the target version but the marker could not be advanced,
    /// so the Package still declares the source version. Rerunning skips the finished
    /// artifacts and retries the marker.
    MarkerAdvanceFailed {
        path: PathBuf,
        error: ValidatedReplacementError<E, UpgradeCandidateFailure>,
    },
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
}

pub type MetadataUpgradeReport<'a, E> = MutationReport<
    MetadataUpgradePlan<'a>,
    MetadataUpgradeResult,
    MetadataUpgradeRejection<E>,
    MetadataUpgradeWarning<E>,
>;

pub fn commit_metadata_upgrade<'a, F: FileSystem>(
    plan: MetadataUpgradePlan<'a>,
    filesystem: &F,
) -> MetadataUpgradeReport<'a, F::Error> {
    if plan.source_version == plan.target_version {
        let result = MetadataUpgradeResult {
            source_version: plan.source_version,
            reached_version: plan.source_version,
            upgraded: Vec::new(),
            skipped: Vec::new(),
            marker_advanced: false,
            index: None,
        };
        return MutationReport::Unchanged { plan, result };
    }

    let chain = plan.chain.clone();
    let mut upgraded = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();

    for artifact in plan.artifacts.clone() {
        if artifact.state == UpgradeArtifactState::Blocked {
            warnings.push(MetadataUpgradeWarning::PausedAtBlockedArtifact {
                path: artifact.path.clone(),
            });
            break;
        }

        // The plan's classification is evidence read before this run started, not
        // authority over what to do now. Acting on it directly would hand the chain a
        // record another resume has since finished — applying one migration twice —
        // or skip a record that has since gone back to the source version, advancing
        // the marker over something that never reached the target.
        let Ok(bytes) = filesystem.read(&artifact.path) else {
            warnings.push(MetadataUpgradeWarning::PausedAtBlockedArtifact {
                path: artifact.path.clone(),
            });
            break;
        };
        match classify(
            artifact.artifact,
            &bytes,
            plan.source_version,
            plan.target_version,
        ) {
            // Already at the target: a resumed run leaves it exactly as it is.
            UpgradeArtifactState::AlreadyUpgraded => {
                skipped.push(artifact.path.clone());
                continue;
            }
            UpgradeArtifactState::Blocked => {
                warnings.push(MetadataUpgradeWarning::PausedAtBlockedArtifact {
                    path: artifact.path.clone(),
                });
                break;
            }
            UpgradeArtifactState::Pending => {}
        }

        let prior = bytes.clone();
        let mut candidate = bytes;
        let mut refused = false;
        let mut unusable = None;
        for step in &chain {
            match step.transform(artifact.artifact, &candidate) {
                Ok(next) => candidate = next,
                Err(TransformationRefused) => {
                    refused = true;
                    break;
                }
            }
            // Each step declares exactly the next version, and what it produced has to
            // say so. Intermediate versions have no schema in this repository, so this
            // is the whole of what can be asked of them — and it is what separates a
            // step that migrated from one that quietly did nothing, downgraded, or
            // skipped ahead. Carrying such an output forward would end with the marker
            // advancing over an artifact that never left the version it came in at.
            if declared_version_in(&candidate, "schema_version") != Some(step.source_version() + 1)
            {
                unusable = Some(UpgradeArtifactFailure::WrongDeclaredVersion);
                break;
            }
        }
        if refused {
            warnings.push(MetadataUpgradeWarning::PausedAtRefusedTransformation {
                path: artifact.path.clone(),
            });
            break;
        }

        // What the chain produced is judged before anything is staged. An output this
        // build will not write is a run the user has to resolve, not a failed write,
        // and writing it anyway would let the marker advance over an artifact that does
        // not obey the version it now declares.
        let judged = unusable.map_or_else(
            || artifact_is_valid_at(artifact.artifact, &candidate, plan.target_version),
            Err,
        );
        if let Err(failure) = judged {
            warnings.push(MetadataUpgradeWarning::PausedAtUnusableOutput {
                path: artifact.path.clone(),
                failure,
            });
            break;
        }

        // One artifact at a time, atomically. An interruption after this point leaves
        // it upgraded, which is exactly what makes the run resumable.
        //
        // `prior` is the artifact as this run read it a few lines above, before
        // transforming it. Carrying it refuses an artifact someone else rewrote while
        // this upgrade was running, rather than replacing their work with a
        // transformation of bytes that are no longer there.
        if let Err(error) = replace_validated_file_expecting(
            &artifact.path,
            &candidate,
            DestinationExpectation::Exactly(&prior),
            filesystem,
            // The candidate was judged above, before anything could be staged, so
            // nothing this seam could reject reaches it.
            |_| Ok::<(), UpgradeCandidateFailure>(()),
        ) {
            let path = artifact.path.clone();
            // Nothing durable has happened yet, so refusing is the honest report and
            // prior canonical bytes are exactly as they were.
            if upgraded.is_empty() {
                return MutationReport::Rejected {
                    plan,
                    rejection: MetadataUpgradeRejection::ArtifactReplacementFailed { path, error },
                };
            }
            // Earlier artifacts are already upgraded on disk. Calling this a rejection
            // would tell the caller nothing happened when several artifacts had in
            // fact been migrated, and would discard the list of which ones.
            warnings.push(MetadataUpgradeWarning::StoppedAtFailedArtifact { path, error });
            break;
        }
        upgraded.push(artifact.path.clone());
    }

    // The marker advances only when every artifact has reached the target, so an
    // interrupted upgrade never declares itself complete.
    let paused = !warnings.is_empty();
    if paused {
        let result = MetadataUpgradeResult {
            source_version: plan.source_version,
            reached_version: plan.source_version,
            upgraded,
            skipped,
            marker_advanced: false,
            index: None,
        };
        return MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        };
    }

    let marker = format!("{{\n  \"metadata_version\": {}\n}}\n", plan.target_version).into_bytes();
    // The plan read this marker to learn which version everything else was being
    // carried from. Advancing it whatever it now holds would discard a marker somebody
    // else had since written — and the marker is the single fact that says whether this
    // Package may be used at all.
    //
    // The question asked of it is what it means, not how it was spelled: a managed JSON
    // record is canonical by parsed meaning, so a marker still declaring the source
    // version is the marker this run observed however it was reformatted.
    let still_the_source_version = |current: &[u8]| {
        declared_version_in(current, "metadata_version") == Some(plan.source_version)
    };
    if let Err(error) = replace_validated_file_expecting(
        &plan.marker_path,
        &marker,
        DestinationExpectation::Equivalent(&still_the_source_version),
        filesystem,
        |_| Ok::<(), UpgradeCandidateFailure>(()),
    ) {
        let path = plan.marker_path.clone();
        if upgraded.is_empty() {
            return MutationReport::Rejected {
                plan,
                rejection: MetadataUpgradeRejection::MarkerAdvanceFailed { path, error },
            };
        }
        // Every artifact reached the target and only the marker did not. The Package
        // still declares the source version, which is what makes rerunning safe, but
        // the artifacts really were rewritten and the report has to say so.
        let result = MetadataUpgradeResult {
            source_version: plan.source_version,
            reached_version: plan.source_version,
            upgraded,
            skipped,
            marker_advanced: false,
            index: None,
        };
        return MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings: vec![MetadataUpgradeWarning::MarkerAdvanceFailed { path, error }],
        };
    }

    let index = match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => Some(package.index().clone()),
        PackageLoad::IndexReplacementFailed(failed) => {
            warnings.push(MetadataUpgradeWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            Some(failed.canonical_evidence().index().clone())
        }
        PackageLoad::OperationFailed(failed) => {
            warnings.push(MetadataUpgradeWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            None
        }
        _ => {
            warnings.push(MetadataUpgradeWarning::RefreshUnavailable);
            None
        }
    };

    let result = MetadataUpgradeResult {
        source_version: plan.source_version,
        reached_version: plan.target_version,
        upgraded,
        skipped,
        marker_advanced: true,
        index,
    };
    if warnings.is_empty() {
        MutationReport::Committed { plan, result }
    } else {
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        }
    }
}
