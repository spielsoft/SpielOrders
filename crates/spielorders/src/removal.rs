//! Planning and persistence for one explicit whole-Order removal, to the Package's
//! Removed Orders Directory. A front end that moves the removed directory on to the
//! platform Trash does so afterwards, as its own act.
//!
//! Supporting Material moves only as part of one intact Order Directory. Nothing here
//! enumerates, reads, or rewrites a directory's children, and nothing here erases anything.

use crate::{
    Answer, Failure, Warning,
    canonical::{CurrentPackageCandidate, HealthyOrder, Touched},
    evidence::EvidenceFingerprint,
    filesystem::{DestinationExpectation, replace_validated_file_expecting},
    metadata::{OrderDocument, PackageRoot},
    mutation::{
        CleanupFailure, CommitFailure, IndexRefresh, MutationReport, Never, OperationKind,
        PlannedOperation,
    },
    operations::FailedOperation,
    order_edits::resolve_order,
    ports::{Effects, FileSystem, FileSystemEntryKind, RenameOutcome},
    readme::{Location, package_relative_path_text},
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
    pub(crate) operations: Vec<PlannedOperation>,
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

        let mut operations = Vec::new();
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
            .map(|destination| located(&root, destination))
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
        let locator = locator_outcome(removed_relative_depth + 1, order.package_root());
        if creates {
            operations.push(PlannedOperation::new(
                OperationKind::CreateRemovedOrdersDirectory,
                removed.clone(),
            ));
        }
        operations.push(PlannedOperation::new(
            OperationKind::MoveOrderDirectory,
            destination.clone(),
        ));
        if matches!(locator, LocatorOutcome::Updated { .. }) {
            operations.push(PlannedOperation::new(
                OperationKind::ReplaceOrderRecord,
                located(&root, order_record(&destination.absolute_path)),
            ));
        }
        operations.push(PlannedOperation::new(
            OperationKind::RefreshPackageIndex,
            index,
        ));

        let evidence = removal_evidence(&package, &order);
        Ok(OrderRemovalPlan {
            root,
            source,
            destination,
            removed_directory: removed,
            creates_removed_orders_directory: creates,
            locator,
            document,
            evidence,
            operations,
        })
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
/// Package load judged healthy and [`HealthyOrder::package_root`] is where that Order's
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

pub(crate) fn commit_order_removal<E: Effects>(
    plan: OrderRemovalPlan,
    effects: &E,
) -> Answer<OrderRemovalReport> {
    let filesystem = effects.file_system();
    Answer::of(|| {
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

        let mut warnings = Vec::new();
        let destination = plan.destination.clone();
        // What this operation itself creates, and therefore the only thing its own
        // rejection may undo. A directory that was already there is never recorded
        // here and so is never a candidate.
        let mut created = None;
        if plan.creates_removed_orders_directory {
            let removed = &plan.removed_directory;
            if filesystem.create_directory(&removed.absolute_path).is_err() {
                let failure = CommitFailure::new(removed.clone());
                return Err(rejected(failure, Vec::new()));
            }
            created = Some(removed.clone());
        }
        // One rename moves the complete tree. Nothing copies, enumerates, or
        // rewrites a child, so README, managed metadata, and Supporting Material
        // travel byte-identical.
        let (failure, error) =
            match filesystem.rename_new(&plan.source.absolute_path, &destination.absolute_path) {
                Ok(RenameOutcome::Renamed) => (None, None),
                Ok(RenameOutcome::DestinationExists) => {
                    (Some(CommitFailure::new(destination.clone())), None)
                }
                Err(error) => (Some(CommitFailure::new(destination.clone())), Some(error)),
            };
        if let Some(failure) = failure {
            let cleanup_failures = undo_created_directory(&plan.root, created, filesystem);
            let rejection = rejected(failure, cleanup_failures);
            return Err(match error {
                Some(error) => rejection.unless_unsupported(
                    &plan.root,
                    &plan.source.absolute_path,
                    &error,
                    filesystem,
                ),
                None => rejection,
            });
        }
        // The locator is rewritten only after the directory has left active
        // scope, so a failure here cannot damage the Package the caller still
        // uses.
        if let LocatorOutcome::Updated { to } = &plan.locator
            && let Err(failure) = rewrite_locator(
                &plan.root,
                &destination.absolute_path,
                &plan.document,
                to,
                filesystem,
            )
        {
            warnings.push(Warning::LocatorUpdateFailed(failure));
        }

        let touched = Touched::orders([plan.source.absolute_path.clone()]);
        let refresh = IndexRefresh::after_commit(&plan.root, &touched, filesystem);
        warnings.extend(refresh.warning());

        let result = OrderRemovalResult {
            final_location: destination,
        };
        // The directory has already gone. A warning describes what could not be refreshed
        // afterwards, never a reason to retry the removal itself.
        Ok(MutationReport::committed(result, warnings))
    })
}

/// Undoes the one Removed Orders Directory this operation created, after the removal
/// that created it was rejected before anything of the Order moved into it.
///
/// It removes only that directory, and only while it is still the empty ordinary
/// directory this operation created. A pre-existing directory is never a candidate,
/// because `created` is `None` for one. Anything that arrived inside it in the meantime
/// is evidence this operation did not create, so its presence retains the directory
/// rather than being swept up with it — the whole point of undoing the creation is to
/// leave the Package as the rejected removal found it, not to take something else away.
fn undo_created_directory<F: FileSystem + FileSystem>(
    _root: &Path,
    created: Option<Location>,
    filesystem: &F,
) -> Vec<CleanupFailure> {
    let Some(location) = created else {
        return Vec::new();
    };
    let path = location.absolute_path.clone();
    // Anything but the empty directory this operation created is not what it created,
    // and this rollback does not remove it; what it cannot observe it cannot remove either.
    let retained = || vec![CleanupFailure::new(location.clone())];
    match filesystem.symlink_metadata(&path) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        // Already gone: nothing this operation created is left to undo.
        Ok(None) => return Vec::new(),
        Ok(Some(_)) => return retained(),
        Err(_) => return retained(),
    }
    // Emptiness is established before the removal rather than inferred from it, so an
    // arrival this rollback can see is named as retained evidence instead of surfacing
    // as an unexplained removal error. This narrows that window rather than closing it:
    // something arriving inside it still makes the removal fail, which is reported as a
    // failed cleanup naming a directory that really is still there.
    match filesystem.read_directory(&path) {
        Ok(entries) if entries.is_empty() => {}
        Ok(_) => return retained(),
        Err(_) => return retained(),
    }
    match filesystem.remove_directory(&path) {
        Ok(()) => Vec::new(),
        Err(_) => vec![CleanupFailure::new(location)],
    }
}

fn rewrite_locator<F: FileSystem>(
    root: &Path,
    destination: &Path,
    document: &OrderDocument,
    locator: &str,
    filesystem: &F,
) -> Result<(), Failure> {
    let record = order_record(destination);
    let Some(candidate) = relocated_record(document, locator) else {
        return Err(Failure::commit(
            CommitFailure::new(located(root, record)),
            Vec::new(),
        ));
    };
    // The record this plan confirmed is the one the rewrite is authorized to replace,
    // and the copy it replaces is that record's own, carried here by the move. This is
    // the only guard over it: the Order has left the caller's view, and nothing looks
    // at that record again. Meaning is what is compared, not bytes — the rule that
    // sends a managed Order record replaced one at a time to `Equivalent` is in
    // `docs/architecture/durable-writes.md`.
    //
    // Projected the way the load projects it, because `document` is the load's own
    // projection rather than a raw parse: a record whose stored facts carry untrimmed
    // text projects to the same Order, and comparing against the raw parse would
    // refuse to relocate a record a person had tidied by hand.
    let still_holds_the_planned_record = |current: &[u8]| {
        OrderDocument::from_json_bytes(current)
            .ok()
            .and_then(|parsed| parsed.validate().ok())
            .is_some_and(|projected| projected.order() == document)
    };
    let replacement = replace_validated_file_expecting(
        &record,
        &candidate,
        DestinationExpectation::Equivalent(&still_holds_the_planned_record),
        filesystem,
        |_| Ok::<(), Never>(()),
    );
    replacement.map_err(|error| Failure::of_replacement(root, &record, error))
}

/// The plan's own confirmed record with its technical locator replaced, emitted as
/// this application's canonical bytes.
///
/// The round trip through the parse is what makes the result a validated candidate
/// rather than an assembled one: the replaced value has to be readable back as a
/// locator before anything is staged, which is asked of the field itself because the
/// field carries that verdict. `None` therefore means the plan's own record stopped
/// round-tripping, which is an internal bug rather than a state of the Package — and
/// after a whole directory has already moved, reporting it is a better answer than
/// ending the process.
fn relocated_record(document: &OrderDocument, locator: &str) -> Option<Vec<u8>> {
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

/// One planned path, as this Package locates it.
///
/// The Package-relative spelling holds by construction, as it does for a planned creation
/// artifact: the source is a discovered Order Directory whose own `Location` the load
/// already carries, the Removed Orders Directory is a configured root child the load has
/// already confirmed has one, and the destination is that directory joined with the
/// source's own basename or with this module's literal managed names.
fn located(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned removal path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned removal path is normalized")
}

/// The managed Order record inside one Order Directory.
///
/// One spelling for the operation this removal plans and the file it later replaces, so
/// the path it reports and the path it writes cannot come apart.
fn order_record(directory: &Path) -> PathBuf {
    directory.join(".spielorders").join("order.json")
}

fn rejected(failure: CommitFailure, cleanup_failures: Vec<CleanupFailure>) -> Failure {
    Failure::commit(failure, cleanup_failures)
}

fn preconditions_changed(location: Location) -> Failure {
    Failure::PreconditionsChanged { location }
}
