//! Applying exactly one currently diagnosed, user-confirmed repair: one managed artifact
//! of a Package's own at Package scope or an Order's record at Order scope, or one of the
//! decided semantic resolutions of an Order, which `semantic_repair` carries out.
//!
//! A repair executes only what its scope's own diagnosis authorized, for one case at a
//! time, against evidence it revalidates immediately before acting. It never salvages
//! malformed JSON, guesses a missing fact, selects among staging candidates, or cleans
//! anything up on its own.
//!
//! **Which candidate is safe is decided here, per artifact kind.** Each managed artifact
//! is held to its own schema and its own semantics before a byte is written, and an Order
//! record also to the Package constraints no record can be judged against from its own
//! bytes. An adapter judging that would be a second place deciding what this core may
//! publish.

use crate::{
    Answer, Failure, Warning,
    canonical::{OrderPaths, load_package},
    diagnosis::{self, Breadth, Diagnosis},
    filesystem::{
        DestinationExpectation, promote_staged_file, replace_validated_file_expecting, staging_path,
    },
    metadata::{
        Config, IndexDocument, OrderDocument, OrderId, VendorId, VendorsDocument, VersionDocument,
    },
    mutation::{IndexRefresh, MutationReport, Never, OperationKind, PlannedOperation},
    operations::FailedOperation,
    package::vendor_aggregate_is_package_valid,
    paths::{locator_resolves_to_root, resolve_package_layout},
    ports::FileSystem,
    readme::{Location, located_in_package, rooted_at},
    repair_cases::RepairResolution,
    semantic_repair::{self, SemanticRepairPlan, SemanticResolution},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairScope {
    /// A SpielOrders Root's own managed artifacts.
    Package,
    /// One Order Directory's managed record.
    Order,
}

/// Order on its own.
///
/// Each of these is a cross-record fact, so no record can be judged against them from
/// its own bytes. A repair that skipped them would answer one diagnosed condition by
/// creating another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageConstraint {
    /// A different Order Directory of this Package already holds this Order ID.
    OrderIdentityTaken,
    /// The Vendor Reference names no Vendor of this Package.
    VendorReferenceUnresolved,
    /// The technical locator does not lead from this Order Directory to this
    /// Package's SpielOrders Root.
    PackageRootMismatch,
}

/// Which managed artifact a candidate would become.
///
/// The kind decides the judgement, so it is resolved from the destination's own managed
/// name rather than supplied. Which artifacts a repair may reach at all is its scope's
/// diagnosis's answer: a case is looked up only there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedArtifact {
    /// `.spielorders/version.json` — the Package marker.
    Marker,
    /// `.spielorders/config.toml` — the configured managed paths.
    Configuration,
    /// `.spielorders/vendors.json` — the complete Vendor aggregate.
    VendorAggregate,
    /// `.spielorders/index.json` — the derived Package Index.
    DerivedIndex,
    /// An Order Directory's `.spielorders/order.json`.
    OrderRecord,
}

impl ManagedArtifact {
    fn of(destination: &Path) -> Option<Self> {
        match destination.file_name()?.to_str()? {
            "version.json" => Some(Self::Marker),
            "config.toml" => Some(Self::Configuration),
            "vendors.json" => Some(Self::VendorAggregate),
            "index.json" => Some(Self::DerivedIndex),
            "order.json" => Some(Self::OrderRecord),
            _ => None,
        }
    }
}

/// What the diagnosis found at the destination this repair writes, carried into the
/// write as its changed-destination evidence: exact bytes, or observed absence. See
/// Durable managed-file writes.
#[derive(Clone, Debug, Eq, PartialEq)]
enum DiagnosedDestination {
    Absent,
    Content(Vec<u8>),
}

impl DiagnosedDestination {
    fn expectation(&self) -> DestinationExpectation<'_> {
        match self {
            Self::Absent => DestinationExpectation::Absent,
            Self::Content(observed) => DestinationExpectation::Exactly(observed),
        }
    }
}

/// One complete, validated repair of one diagnosed case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactRepairPlan {
    scope: RepairScope,
    directory: Location,
    root: PathBuf,
    artifact: ManagedArtifact,
    destination: Location,
    candidate: Vec<u8>,
    observed: DiagnosedDestination,
    source: Option<Location>,
    pub(crate) operations: Vec<PlannedOperation>,
}

impl ArtifactRepairPlan {
    /// The scope's own directory: the SpielOrders Root at Package scope, the Order
    /// Directory at Order scope.
    pub fn directory(&self) -> &Path {
        &self.directory.absolute_path
    }

    /// The SpielOrders Root the scope belongs to.
    ///
    /// Planning resolved it, so every refusal raised from this plan onward has a Root to
    /// name whether or not the value it refuses over carries one of its own.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The single managed artifact this repair replaces.
    pub fn destination(&self) -> &Path {
        &self.destination.absolute_path
    }

    /// The staging candidate a promotion reads its bytes from, when the action is a
    /// promotion.
    pub fn source(&self) -> Option<&Path> {
        self.source
            .as_ref()
            .map(|location| location.absolute_path.as_path())
    }
}

/// One complete, validated repair of one diagnosed case: of a managed artifact, or of an
/// Order's meaning.
#[derive(Clone, Debug, PartialEq)]
pub enum RepairPlan {
    Artifact(Box<ArtifactRepairPlan>),
    Semantic(Box<SemanticRepairPlan>),
}

impl RepairPlan {
    /// The scope's own directory: the SpielOrders Root at Package scope, the Order
    /// Directory at Order scope.
    pub fn directory(&self) -> &Path {
        match self {
            Self::Artifact(plan) => plan.directory(),
            Self::Semantic(plan) => plan.directory(),
        }
    }

    /// The SpielOrders Root the scope belongs to, which every refusal raised from this
    /// plan onward names.
    pub fn root(&self) -> &Path {
        match self {
            Self::Artifact(plan) => plan.root(),
            Self::Semantic(plan) => plan.root(),
        }
    }

    fn scope(&self) -> RepairScope {
        match self {
            Self::Artifact(plan) => plan.scope,
            Self::Semantic(_) => RepairScope::Order,
        }
    }
}

/// Plans exactly one confirmed repair in `scope`, rooted at `selected` — the SpielOrders
/// Root at Package scope, the Order Directory at Order scope — revalidating the case and
/// the supplied facts against current evidence before proposing any effect.
pub(crate) fn plan_repair<F: FileSystem>(
    scope: RepairScope,
    selected: &Path,
    handle: &str,
    resolution: &RepairResolution,
    filesystem: &F,
) -> Answer<RepairPlan> {
    Answer::of(|| {
        let diagnosis = diagnose(scope, selected, filesystem)?;
        // The Package the diagnosis was taken against, never one inferred by counting
        // parents: the Orders Directory is configured and may sit at any depth.
        let root = diagnosis.context.root.absolute_path.clone();
        let directory = match scope {
            RepairScope::Package => root.clone(),
            RepairScope::Order => diagnosis.examined_orders[0]
                .directory
                .absolute_path
                .to_path_buf(),
        };
        let in_scope = || rooted_at(&root, directory.clone());
        let action = resolution.action();
        let unauthorized = || Failure::UnauthorizedAction {
            handle: handle.to_owned(),
            action,
            scope: in_scope(),
        };
        let case = diagnosis.case(handle).ok_or_else(|| Failure::UnknownCase {
            handle: handle.to_owned(),
            scope: in_scope(),
        })?;
        if !case.actions.contains(&action) {
            return Err(unauthorized());
        }
        let evidence = case.location.absolute_path.clone();
        let semantic = match resolution {
            RepairResolution::ReplaceManagedRecord { record } => {
                return plan_artifact_repair(
                    scope,
                    directory,
                    root,
                    Offered::Supplied(evidence, record),
                    filesystem,
                )
                .map(|plan| RepairPlan::Artifact(Box::new(plan)));
            }
            RepairResolution::PromoteStagingCandidate => {
                return plan_artifact_repair(
                    scope,
                    directory,
                    root,
                    Offered::Staged(evidence),
                    filesystem,
                )
                .map(|plan| RepairPlan::Artifact(Box::new(plan)));
            }
            RepairResolution::AssignExistingVendor { name } => {
                SemanticResolution::AssignExistingVendor(name)
            }
            RepairResolution::CreateAndAssignVendor { name } => {
                SemanticResolution::CreateAndAssignVendor(name)
            }
            RepairResolution::ClearVendorReference => SemanticResolution::ClearVendorReference,
            RepairResolution::AdoptNewOrderIdentity => SemanticResolution::AdoptNewOrderIdentity,
            RepairResolution::RepairPackageLocator => SemanticResolution::RepairPackageLocator,
        };
        // A Package's own cases offer no semantic action, so the authorization above has
        // already refused one at Package scope.
        if scope == RepairScope::Package {
            return Err(unauthorized());
        }
        semantic_repair::plan(&root, &directory, semantic, filesystem)
            .map(|plan| RepairPlan::Semantic(Box::new(plan)))
    })
}

/// Plans the replacement or promotion of the one managed artifact the authorized case
/// was diagnosed at.
fn plan_artifact_repair<F: FileSystem>(
    scope: RepairScope,
    directory: PathBuf,
    root: PathBuf,
    offered: Offered<'_>,
    filesystem: &F,
) -> Result<ArtifactRepairPlan, Failure> {
    let in_scope = || rooted_at(&root, directory.clone());
    let (destination, source) = match &offered {
        Offered::Supplied(evidence, _) => (evidence.clone(), None),
        // The diagnosed candidate is the only one that may be promoted, and it is reread
        // and validated now rather than trusted from the diagnosis.
        Offered::Staged(staged) => {
            let destination =
                promoted_destination(staged).ok_or_else(|| Failure::InvalidCandidate {
                    candidate: rooted_at(&root, staged.clone()),
                })?;
            (destination, Some(staged.clone()))
        }
    };
    let artifact =
        ManagedArtifact::of(&destination).ok_or_else(|| Failure::UnrecognizedArtifact {
            destination: rooted_at(&root, destination.clone()),
        })?;
    let read;
    let bytes = match &offered {
        Offered::Supplied(_, record) => record.as_slice(),
        Offered::Staged(staged) => {
            read = filesystem.read(staged).map_err(|error| {
                Failure::from(FailedOperation::inside(
                    &root,
                    staged.clone(),
                    &error,
                    filesystem,
                ))
            })?;
            read.as_slice()
        }
    };

    // A candidate is accepted only as a complete, fully valid artifact of its own kind.
    // Unknown or invalid facts disappear only because the user confirmed an artifact
    // without them, never because this core removed them.
    let candidate_location = || match &source {
        Some(staged) => rooted_at(&root, staged.clone()),
        None => in_scope(),
    };
    let (candidate, record) =
        validated_candidate(artifact, bytes, &root, filesystem).ok_or_else(|| {
            Failure::InvalidCandidate {
                candidate: candidate_location(),
            }
        })?;
    // Valid as an Order says nothing about the Package it lands in. A record whose
    // identity, Vendor Reference, or locator breaks a cross-record constraint would
    // answer this diagnosed condition by creating another.
    if let Some(record) = record
        && let Some(constraint) =
            package_evidence(&root, &directory, filesystem)?.broken_by(&record)
    {
        return Err(Failure::CandidateDamagesPackage {
            constraint,
            candidate: candidate_location(),
        });
    }
    if source.is_none() {
        refuse_staging_evidence(&root, &destination, filesystem)?;
    }
    let observed = diagnosed_destination(&root, &destination, filesystem)?;

    let kind = match source {
        Some(_) => OperationKind::PromoteStagingCandidate,
        None => OperationKind::ReplaceManagedArtifact,
    };
    let operations = vec![
        PlannedOperation::new(kind, located_in_package(&root, destination.clone())),
        PlannedOperation::new(
            OperationKind::RefreshPackageIndex,
            located_in_package(&root, root.join(".spielorders").join("index.json")),
        ),
    ];
    Ok(ArtifactRepairPlan {
        scope,
        directory: located_in_package(&root, directory),
        artifact,
        destination: located_in_package(&root, destination),
        candidate,
        observed,
        source: source.map(|source| located_in_package(&root, source)),
        root,
        operations,
    })
}

/// The candidate a repair publishes: one the caller supplied for the diagnosed artifact,
/// or the diagnosed stage.
enum Offered<'a> {
    Supplied(PathBuf, &'a Vec<u8>),
    Staged(PathBuf),
}

fn diagnose<F: FileSystem>(
    scope: RepairScope,
    selected: &Path,
    filesystem: &F,
) -> Result<Diagnosis, Failure> {
    match scope {
        RepairScope::Package => diagnosis::of_package(selected, Breadth::OwnArtifacts, filesystem),
        RepairScope::Order => diagnosis::of_order(selected, filesystem),
    }
}

/// Refuses a replacement whose own beside-destination stage path is already occupied.
fn refuse_staging_evidence<F: FileSystem>(
    root: &Path,
    destination: &Path,
    filesystem: &F,
) -> Result<(), Failure> {
    let staging = staging_path(destination).ok_or_else(|| Failure::UnrecognizedArtifact {
        destination: rooted_at(root, destination.to_path_buf()),
    })?;
    match filesystem.symlink_metadata(&staging) {
        Ok(None) => Ok(()),
        Ok(Some(_)) => Err(Failure::StagingEvidencePresent {
            staged: rooted_at(root, staging),
        }),
        Err(error) => Err(Failure::from(FailedOperation::inside(
            root, staging, &error, filesystem,
        ))),
    }
}

/// Exactly what stands at the destination now, so the write can refuse anything else.
fn diagnosed_destination<F: FileSystem>(
    root: &Path,
    destination: &Path,
    filesystem: &F,
) -> Result<DiagnosedDestination, Failure> {
    let unreadable = |error: F::Error| {
        Failure::from(FailedOperation::inside(
            root,
            destination.to_path_buf(),
            &error,
            filesystem,
        ))
    };
    let observed = filesystem
        .symlink_metadata(destination)
        .map_err(unreadable)?;
    match observed {
        None => Ok(DiagnosedDestination::Absent),
        Some(_) => filesystem
            .read(destination)
            .map(DiagnosedDestination::Content)
            .map_err(unreadable),
    }
}

/// A staging artifact is named `.<destination>.spielorders-stage` beside the destination
/// it belongs to, so promotion never has to choose among candidates.
fn promoted_destination(staged: &Path) -> Option<PathBuf> {
    let name = staged.file_name()?.to_str()?;
    let name = name.strip_prefix('.')?.strip_suffix(".spielorders-stage")?;
    Some(staged.with_file_name(name))
}

/// Accepts bytes only as a complete artifact of this kind that parses and satisfies that
/// kind's own semantics. Anything less is refused rather than partially salvaged.
///
/// **One judgement per kind**, each the one this repository already publishes for that
/// artifact. An Order record validates as an Order and is returned parsed as well, because
/// the Package constraints are asked of its meaning while the write is of its bytes. The
/// Vendor aggregate is put to [`vendor_aggregate_is_package_valid`]; the configuration is
/// resolved as this Package's layout, because a configured path that escapes the Root or
/// occupies the reserved metadata subtree is exactly what damages a Package; and the
/// marker's declared version is settled by the type it deserializes into.
///
/// The canonical bytes are returned rather than the caller's, so what is published is the
/// document this build round-trips — except the user's own TOML configuration, and the
/// derived index, whose bytes a rebuild owns.
fn validated_candidate<F: FileSystem>(
    artifact: ManagedArtifact,
    bytes: &[u8],
    root: &Path,
    filesystem: &F,
) -> Option<(Vec<u8>, Option<OrderDocument>)> {
    let canonical = match artifact {
        ManagedArtifact::OrderRecord => {
            let document = OrderDocument::from_json_bytes(bytes).ok()?;
            document.validate().ok()?;
            let bytes = document.to_json_bytes().ok()?;
            return Some((bytes, Some(document)));
        }
        ManagedArtifact::Marker => VersionDocument::from_json_bytes(bytes)
            .ok()?
            .to_json_bytes()
            .ok()?,
        ManagedArtifact::Configuration => {
            let config = Config::from_toml_bytes(bytes).ok()?;
            // A filesystem that would not answer is refused for the same reason a broken
            // path is: nothing shows this configuration is usable.
            resolve_package_layout(root, &config, filesystem).ok()?;
            bytes.to_vec()
        }
        ManagedArtifact::VendorAggregate => {
            let vendors = VendorsDocument::from_json_bytes(bytes).ok()?;
            if !vendor_aggregate_is_package_valid(&vendors) {
                return None;
            }
            vendors.to_json_bytes().ok()?
        }
        ManagedArtifact::DerivedIndex => {
            IndexDocument::from_json_bytes(bytes).ok()?;
            bytes.to_vec()
        }
    };
    Some((canonical, None))
}

/// The Package facts a candidate for one Order Directory is judged against.
///
/// Read from the Package rather than from the diagnosis, because a diagnosis answers
/// for one Order scope while these constraints are about the rest of the Package:
/// which identities the other Order Directories hold, and which Vendors exist.
struct PackageEvidence {
    root: PathBuf,
    directory: PathBuf,
    identities_held_elsewhere: Vec<OrderId>,
    vendor_ids: Vec<VendorId>,
}

/// Reads that evidence without changing anything, so it can be consulted while
/// planning.
fn package_evidence<F: FileSystem>(
    root: &Path,
    directory: &Path,
    filesystem: &F,
) -> Result<PackageEvidence, Failure> {
    // The diagnosis this repair answers established a current Package moments ago.
    // Reaching the error means that stopped holding, and a candidate no evidence can
    // clear is refused rather than written, under what the read established.
    let package = load_package(root, filesystem)?;
    let another_order = |paths: &OrderPaths| paths.directory.absolute_path != directory;
    // Every discovered Order Directory except the one being repaired. A quarantined
    // Order still holds the identity it was quarantined for, so consulting the
    // healthy set alone would call a contested identity free.
    let identities_held_elsewhere = package
        .healthy_orders
        .iter()
        .filter(|order| another_order(&order.paths))
        .map(|order| order.document.order_id.clone())
        .chain(
            package
                .damaged_orders
                .iter()
                .filter(|order| another_order(&order.paths))
                .filter_map(|order| order.order_id.clone()),
        )
        .collect();
    let vendor_ids = package
        .vendors
        .iter()
        .map(|vendor| vendor.vendor_id.clone())
        .collect();
    Ok(PackageEvidence {
        root: root.to_path_buf(),
        directory: directory.to_path_buf(),
        identities_held_elsewhere,
        vendor_ids,
    })
}

impl PackageEvidence {
    /// The first Package constraint this candidate breaks, when it breaks one.
    fn broken_by(&self, candidate: &OrderDocument) -> Option<PackageConstraint> {
        if self.identities_held_elsewhere.contains(&candidate.order_id) {
            return Some(PackageConstraint::OrderIdentityTaken);
        }
        if let Some(vendor_id) = &candidate.vendor_id
            && !self.vendor_ids.contains(vendor_id)
        {
            return Some(PackageConstraint::VendorReferenceUnresolved);
        }
        // Asked through the same predicate the Package loader applies to a stored
        // record, so a candidate cannot be accepted on terms the loader would reject —
        // including a candidate carrying no locator this build can follow, which the
        // loader diagnoses as the same mismatch.
        let follows_this_package = candidate
            .package_root
            .spelled()
            .is_some_and(|locator| locator_resolves_to_root(&self.directory, locator, &self.root));
        if !follows_this_package {
            return Some(PackageConstraint::PackageRootMismatch);
        }
        None
    }
}

pub(crate) type RepairReport = MutationReport<()>;

/// Commits one planned repair. Only a semantic resolution that mints an identity draws on
/// `new_uuid`.
pub(crate) fn commit_repair<F: FileSystem>(
    plan: RepairPlan,
    new_uuid: &dyn Fn() -> String,
    filesystem: &F,
) -> Answer<RepairReport> {
    let written = match &plan {
        RepairPlan::Artifact(plan) => write_artifact(plan, filesystem),
        RepairPlan::Semantic(plan) => semantic_repair::write(plan, new_uuid, filesystem),
    };
    if let Err(rejection) = written {
        return Err(rejection).into();
    }

    let refresh = IndexRefresh::after_commit(plan.root(), filesystem);
    let mut warnings = Vec::from_iter(refresh.warning());
    if diagnose(plan.scope(), plan.directory(), filesystem).is_err() {
        warnings.push(Warning::RediagnosisUnavailable);
    }
    Ok(MutationReport::committed((), warnings)).into()
}

fn write_artifact<F: FileSystem>(plan: &ArtifactRepairPlan, filesystem: &F) -> Result<(), Failure> {
    // A promotion rereads its candidate immediately before acting; a stage that
    // changed since planning is no longer the candidate that was diagnosed.
    if let Some(source) = plan.source() {
        match filesystem.read(source) {
            Ok(bytes)
                if validated_candidate(plan.artifact, &bytes, &plan.root, filesystem)
                    .is_some_and(|(candidate, _)| candidate == plan.candidate) => {}
            _ => {
                return Err(Failure::EvidenceChanged {
                    evidence: rooted_at(&plan.root, source.to_path_buf()),
                });
            }
        }
    }

    match plan.source() {
        // Promotion completes the interrupted replacement the stage belongs to.
        Some(source) => promote_staged_file(
            source,
            plan.destination(),
            plan.observed.expectation(),
            filesystem,
        )
        .map_err(|_| Failure::of_promotion(&plan.root, plan.destination())),
        None => replace_validated_file_expecting(
            plan.destination(),
            &plan.candidate,
            plan.observed.expectation(),
            filesystem,
            |_| Ok::<(), Never>(()),
        )
        .map_err(|error| Failure::of_replacement(&plan.root, plan.destination(), error)),
    }
}
