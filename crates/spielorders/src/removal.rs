//! Planning and persistence for one explicit whole-Order removal, to the Package's
//! Removed Orders Directory. A front end that moves the removed directory on to the
//! platform Trash does so afterwards, as its own act.
//!
//! Supporting Material moves only as part of one intact Order Directory. Nothing here
//! enumerates, reads, or rewrites a directory's children, and nothing here erases anything.

use crate::{
    Answer, Failure,
    canonical::{CurrentPackageCandidate, HealthyOrder},
    change::PlannedAct,
    evidence::EvidenceFingerprint,
    journal::{Journal, ManagedRecord, Subject, planned_location},
    metadata::{OrderDocument, PackageRoot},
    mutation::{CommitFailure, MutationReport, OperationKind},
    operations::FailedOperation,
    order_edits::resolve_order,
    ports::{Effects, FileSystem, FileSystemEntryKind},
    readme::Location,
};
use std::path::{Path, PathBuf};

/// What the removal does to the Order's technical Package locator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocatorOutcome {
    /// The removed directory sits at the same depth, so the stored locator stays correct
    /// untouched.
    Unchanged,
    /// The removed directory sits at a different depth, so the locator becomes `to`.
    Updated { to: String },
}

/// One complete, validated removal of one whole Order Directory.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderRemovalPlan {
    root: PathBuf,
    source: Location,
    destination: Location,
    removed_directory: Location,
    creates_removed_orders_directory: bool,
    locator: LocatorOutcome,
    document: OrderDocument,
    evidence: EvidenceFingerprint,
    index: Location,
    pub(crate) journal: Journal,
}

pub(crate) fn plan_order_removal<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Answer<OrderRemovalPlan> {
    Answer::of(|| {
        let (package, order) = resolve_order(order_directory, filesystem)?;
        let root = package.context.root.absolute_path.clone();
        let source = order.paths.directory.clone();
        let document = order.document.clone();
        let index = package.context.index.clone();

        // A resolved Package always names its Removed Orders Directory, and the
        // Package load has already refused a configured path that is anything
        // other than absent or an ordinary root-child directory, so an unusable
        // one damages the Package before a removal is ever planned.
        let removed_relative_depth = package
            .context
            .removed_orders_directory
            .as_ref()
            .and_then(|location| location.package_relative_path.as_deref())
            .map(|relative| relative.split('/').count())
            .expect("a resolved Package names its Removed Orders Directory");
        let removed = package
            .context
            .removed_orders_directory
            .as_ref()
            .cloned()
            .expect("a resolved Package names its Removed Orders Directory");
        // It is created lazily, so it may legitimately be absent.
        let creates = match filesystem.symlink_metadata(&removed.absolute_path) {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(error) => {
                return Err(Failure::from(
                    FailedOperation::filesystem(removed.clone(), &error, filesystem)
                        .under(&package.context.root),
                ));
            }
        };
        // The same projection the evidence re-observes, so plan-time and
        // commit-time answers cannot drift apart through two spellings of one rule.
        let destination = projected_destination(&source.absolute_path, &package)
            .map(|destination| planned_location(&root, destination))
            .ok_or_else(|| Failure::OrderNotFound {
                directory: source.clone().into(),
                root: Some(root.clone()),
            })?;
        if !creates {
            match filesystem.symlink_metadata(&destination.absolute_path) {
                Ok(None) => {}
                Ok(found @ Some(_)) => {
                    return Err(Failure::DestinationCollision {
                        root: package.context.root.clone(),
                        location: destination.clone(),
                        found,
                    });
                }
                Err(error) => {
                    return Err(Failure::from(
                        FailedOperation::filesystem(destination.clone(), &error, filesystem)
                            .under(&package.context.root),
                    ));
                }
            }
        }
        let locator = locator_outcome(removed_relative_depth + 1, order.locator());
        let evidence = removal_evidence(&package, &order);
        let mut plan = OrderRemovalPlan {
            root,
            source,
            destination,
            removed_directory: removed,
            creates_removed_orders_directory: creates,
            locator,
            document,
            evidence,
            journal: Journal::new(&package.context.root.absolute_path),
            index,
        };
        plan.journal = plan.removal_journal();
        Ok(plan)
    })
}

/// The path a removal would move `source` to, projected from the
/// Package's configured Removed Orders Directory as it stands in `package`. `None` when
/// the Package names no usable one or the source has no basename to carry over, which
/// is a refusal and never a move to some fallback.
fn projected_destination(source: &Path, package: &CurrentPackageCandidate) -> Option<PathBuf> {
    let removed = &package
        .context
        .removed_orders_directory
        .as_ref()?
        .absolute_path;
    Some(removed.join(source.file_name()?))
}

/// A path as its exact bytes. A path is not required to be UTF-8, and a comparison
/// that silently repaired one would lose exactly the difference this seam looks for.
fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

/// The facts a removal plan is authorized against, as `order` and `package` stand now.
///
/// A removal's effects are decided by *which* Order is being disposed of and *where*
/// the removal would put it. Neither is visible in the path, so neither is covered
/// by confirming that the path still exists: `symlink_metadata` reports an ordinary
/// directory whether the Order standing there is the planned one or a different one
/// left in its place. Each fact is recorded here and re-observed through
/// [`EvidenceFingerprint::confirm`] immediately before the first effect.
fn removal_evidence(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
) -> EvidenceFingerprint {
    // Read from the resolved Order rather than taken from the plan, because comparing
    // the plan's own copy of its source against itself would confirm nothing.
    let document = &order.document;
    let source = &order.paths.directory.absolute_path;
    let fingerprint = EvidenceFingerprint::new()
        // Identity, because the path is location and not identity: an Order ID is what
        // separates this Order from any other that could stand at the same path.
        .observed("order_id", document.order_id.as_str())
        .observed("source", path_bytes(source))
        // The Package the source belongs to, resolved through the Order's own locator.
        // An Order that has come to belong somewhere else is not this plan's subject
        // even where its identity and its path both still match.
        .observed(
            "package_root",
            path_bytes(&package.context.root.absolute_path),
        );
    let fingerprint = match projected_destination(source, package) {
        Some(destination) => fingerprint.observed("destination", path_bytes(&destination)),
        None => fingerprint.absent("destination"),
    };
    // The record as it stands, because the removal may rewrite the plan's
    // own copy of it at the destination — see `rewrite_locator`, which a depth
    // change reaches. That copy is the only one left once the directory has
    // moved, so a plan bound to identity alone would carry an edit made after
    // planning back out and destroy it. It is recorded only here because only
    // here does anything write it.
    fingerprint.observed("order_record", format!("{document:?}"))
}

/// The locator is a normalized relative path from the Order Directory to its Package
/// root, so it depends only on how deep the directory sits. A removal that keeps
/// the same depth keeps the same locator.
///
/// It takes the locator rather than the record, because a removal plans for an Order the
/// Package load judged healthy and [`HealthyOrder::locator`] is where that Order's
/// locator is already settled.
fn locator_outcome(depth: usize, locator: &PackageRoot) -> LocatorOutcome {
    let projected = vec![".."; depth].join("/");
    if projected == locator.as_str() {
        LocatorOutcome::Unchanged
    } else {
        LocatorOutcome::Updated { to: projected }
    }
}

/// Where the removed Order went.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderRemovalResult {
    /// Where the directory landed, in the Removed Orders Directory.
    pub final_location: Location,
}

pub(crate) type OrderRemovalReport = MutationReport<OrderRemovalResult>;

impl OrderRemovalPlan {
    /// The commit removing this plan's Order: the Removed Orders Directory created where
    /// it is absent, and the whole Order Directory moved into it by one rename. The record
    /// is rewritten with its new locator only after the directory has left active scope,
    /// as tidying up, so a failure there cannot damage the Package the caller still uses.
    fn removal_journal(&self) -> Journal {
        let managed = PlannedAct::Managed;
        let mut journal = Journal::new(&self.root);
        if self.creates_removed_orders_directory {
            journal.create_directory(
                self.removed_directory.absolute_path.clone(),
                managed(OperationKind::CreateRemovedOrdersDirectory),
            );
        }
        // One rename moves the complete tree. Nothing copies, enumerates, or rewrites a
        // child, so README, managed metadata, and Supporting Material travel
        // byte-identical.
        journal.move_entry(
            self.source.absolute_path.clone(),
            self.destination.absolute_path.clone(),
            managed(OperationKind::MoveOrderDirectory),
            Subject::Order(self.source.absolute_path.clone()),
        );
        journal.commit_point();
        if let Some(candidate) = self.relocated() {
            // The record this plan confirmed is the one the rewrite is authorized to
            // replace, and the copy it replaces is that record's own, carried there by the
            // move. Its meaning is what is compared, projected the way the load projects
            // it, so a record a person tidied by hand is still relocated.
            let planned = ManagedRecord::order(
                record_path(&self.destination.absolute_path),
                self.document.clone(),
            );
            journal.replace(
                planned.path().to_path_buf(),
                candidate,
                planned.expectation(),
                managed(OperationKind::ReplaceOrderRecord),
                Subject::Other,
            );
        }
        journal.refreshing(self.index.clone());
        journal
    }

    /// The record with the locator it needs in the Removed Orders Directory, where that
    /// differs from the one it stores.
    fn relocated(&self) -> Option<Vec<u8>> {
        match &self.locator {
            LocatorOutcome::Updated { to } => relocated_record(&self.document, to),
            LocatorOutcome::Unchanged => None,
        }
    }
}

/// The journal committing `plan`, once the Order it was planned against is confirmed to
/// still stand where it did, unchanged, in the same Package.
pub(crate) fn confirmed<F: FileSystem>(
    plan: &OrderRemovalPlan,
    filesystem: &F,
) -> Result<Journal, Failure> {
    // The source must still be the ordinary directory this plan validated. Checked here
    // rather than left to the resolution below, which canonicalizes: a link standing
    // where the directory was would resolve to whatever it points at and then be
    // disposed of in its place.
    match filesystem.symlink_metadata(&plan.source.absolute_path) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        _ => return Err(preconditions_changed(plan.source.clone())),
    }

    // Re-resolve the subject rather than settle for having found something there. A
    // source that no longer resolves to a healthy Order of its own Package is neither
    // this plan's Order nor identifiable as any other.
    let (package, order) = match resolve_order(&plan.source.absolute_path, filesystem) {
        Ok(resolved) => resolved,
        Err(
            Failure::OrderNotFound { .. }
            | Failure::DamagedRecord { .. }
            | Failure::DamagedOrder { .. },
        ) => return Err(preconditions_changed(plan.source.clone())),
        Err(failure) => return Err(failure),
    };

    // Immediately before the first effect, which is the lazy creation of the Removed
    // Orders Directory or the move that follows it.
    let outcome = plan.evidence.confirm(&removal_evidence(&package, &order));
    if !outcome.is_unchanged() {
        return Err(Failure::PlanEvidenceUnconfirmed { outcome });
    }
    // A record that needs a new locator and no longer relocates is an internal bug rather
    // than a state of the Package, refused here, before anything moves.
    if matches!(plan.locator, LocatorOutcome::Updated { .. }) && plan.relocated().is_none() {
        let record = record_path(&plan.destination.absolute_path);
        return Err(Failure::commit(
            CommitFailure::new(planned_location(&plan.root, record)),
            Vec::new(),
        ));
    }
    Ok(plan.removal_journal())
}

pub(crate) fn commit_order_removal<E: Effects>(
    plan: OrderRemovalPlan,
    effects: &E,
) -> Answer<OrderRemovalReport> {
    let filesystem = effects.file_system();
    Answer::of(|| {
        let committed = confirmed(&plan, filesystem)?.run(filesystem)?;
        let refresh = committed.refresh(&plan.root, filesystem);
        let result = OrderRemovalResult {
            final_location: plan.destination.clone(),
        };
        // The directory has already gone. A warning describes what could not be refreshed
        // afterwards, never a reason to retry the removal itself.
        Ok(MutationReport::committed(
            result,
            committed.warnings(refresh),
        ))
    })
}

/// The plan's own confirmed record with its technical locator replaced, emitted as
/// this application's canonical bytes.
///
/// The round trip through the parse is what makes the result a validated candidate
/// rather than an assembled one: the replaced value has to be readable back as a
/// locator before anything is staged, which is asked of the field itself because the
/// field carries that verdict. `None` therefore means the plan's own record stopped
/// round-tripping, which is an internal bug rather than a state of the Package, and is
/// refused before anything moves.
pub(crate) fn relocated_record(document: &OrderDocument, locator: &str) -> Option<Vec<u8>> {
    let mut object = serde_json::to_value(document).ok()?.as_object().cloned()?;
    object.insert(
        "package_root".into(),
        serde_json::Value::String(locator.to_owned()),
    );
    let bytes = serde_json::to_vec(&serde_json::Value::Object(object)).ok()?;
    let relocated = OrderDocument::from_json_bytes(&bytes).ok()?;
    relocated.package_root.spelled()?;
    relocated.to_json_bytes().ok()
}

/// The managed Order record inside one Order Directory.
///
/// One spelling for the operation this removal plans and the file it later replaces, so
/// the path it reports and the path it writes cannot come apart.
fn record_path(directory: &Path) -> PathBuf {
    directory.join(".spielorders").join("order.json")
}

fn preconditions_changed(location: Location) -> Failure {
    Failure::PreconditionsChanged { location }
}
