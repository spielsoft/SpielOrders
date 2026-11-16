//! Merging Orders in precedence order: the first Order named is kept, and each later one
//! only fills what the result still lacks — its facts, its Items, and the entries of its
//! directory — before what remains of it is removed as an Order removal removes one.
//!
//! Decision sources: Define the Order record and lifecycle contract; Define the SpielOrders
//! package topology; Durable managed-file writes.

use crate::{
    Answer, Failure, Warning,
    canonical::{CurrentPackageCandidate, HealthyOrder, Touched, load_package_to_change},
    filesystem::{DestinationExpectation, replace_validated_file_expecting},
    metadata::OrderDocument,
    mutation::{IndexRefresh, MutationReport, Never, OperationKind, PlannedOperation},
    operations::{FailedOperation, FailedPath},
    ports::{Effects, FileSystem, FileSystemEntryKind, RenameOutcome},
    readme::{Location, located_in_package},
    removal::{commit_order_removal, plan_order_removal},
    values::same_value,
};
use std::path::{Path, PathBuf};

/// What a merge does with one entry of a merged Order's directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryAction {
    /// Moved into the kept Order at the same relative path, which held nothing.
    Move,
    /// Left behind, because the kept Order already holds that path, and removed with the
    /// merged Order.
    Discard,
}

/// One entry of a merged Order's directory and what the merge does with it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeEntry {
    pub action: EntryAction,
    /// Where the entry is in the merged Order.
    pub from: PathBuf,
    /// Where a moved entry lands in the kept Order.
    pub to: PathBuf,
    pub directory: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderMergePlan {
    root: PathBuf,
    kept: Location,
    merged: Vec<PathBuf>,
    prior: OrderDocument,
    merged_documents: Vec<OrderDocument>,
    candidate: OrderDocument,
    pub(crate) entries: Vec<MergeEntry>,
    pub(crate) operations: Vec<PlannedOperation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderMergeResult {
    /// The kept Order's Directory.
    pub directory: Location,
}

/// The kept Order and the merged ones, as one load of the Package holds them, or why not.
fn resolved<'a>(
    package: &'a CurrentPackageCandidate,
    orders: &[PathBuf],
) -> Result<Vec<&'a HealthyOrder>, Failure> {
    let mut found = Vec::new();
    for directory in orders {
        let order = package
            .healthy_orders
            .iter()
            .find(|order| order.paths.directory.absolute_path == *directory)
            .ok_or_else(|| Failure::PreconditionsChanged {
                location: located_in_package(
                    &package.context.root.absolute_path,
                    directory.clone(),
                ),
            })?;
        if found
            .iter()
            .any(|seen: &&HealthyOrder| seen.document.order_id == order.document.order_id)
        {
            return Err(Failure::InvalidFacts);
        }
        found.push(order);
    }
    if found.len() < 2 {
        return Err(Failure::InvalidFacts);
    }
    Ok(found)
}

/// `kept` with each fact it lacks taken from `merged`, and each of `merged`'s Items it does
/// not already hold added after its own. An Order Group comes only with its Project, or to
/// an Order in the same Project.
fn fill_order(kept: &mut OrderDocument, merged: &OrderDocument) {
    fn fill<T: Clone>(kept: &mut Option<T>, merged: &Option<T>) {
        if kept.is_none() {
            kept.clone_from(merged);
        }
    }
    fill(&mut kept.vendor_id, &merged.vendor_id);
    fill(&mut kept.orderer, &merged.orderer);
    fill(&mut kept.ordering_institution, &merged.ordering_institution);
    fill(&mut kept.currency, &merged.currency);
    let same_project = match (&kept.project, &merged.project) {
        (None, None) => true,
        (Some(kept), Some(merged)) => same_value(kept, merged),
        _ => false,
    };
    if kept.project.is_none() && merged.project.is_some() {
        kept.project.clone_from(&merged.project);
        fill(&mut kept.order_group, &merged.order_group);
    } else if same_project {
        fill(&mut kept.order_group, &merged.order_group);
    }
    for item in &merged.items {
        if !kept.items.contains(item) {
            kept.items.push(item.clone());
        }
    }
}

/// Where an entry already stands in the kept Order as the merge leaves it so far: on disk,
/// or moved there from an earlier merged Order.
fn kind_at<F: FileSystem>(
    kept: &Path,
    relative: &Path,
    moved: &[(PathBuf, PathBuf)],
    filesystem: &F,
) -> Result<Option<FileSystemEntryKind>, F::Error> {
    // A later move may fill a missing child of an earlier moved directory. Probe
    // that more-specific virtual entry before its parent's original contents.
    for (moved_relative, source) in moved.iter().rev() {
        if let Ok(rest) = relative.strip_prefix(moved_relative) {
            let mut path = source.clone();
            path.extend(rest.components());
            return filesystem.symlink_metadata(&path);
        }
        let mut requested = relative.components();
        let Some(root) = source.ancestors().nth(moved_relative.components().count()) else {
            continue;
        };
        let mut actual = root.to_path_buf();
        let mut matches = true;
        for component in moved_relative.components() {
            let Some(candidate) = requested.next() else {
                matches = false;
                break;
            };
            if candidate != component
                && (!may_alias(candidate.as_os_str(), component.as_os_str())
                    || filesystem
                        .entry_name(&actual.join(candidate.as_os_str()))?
                        .as_deref()
                        != Some(component.as_os_str()))
            {
                matches = false;
                break;
            }
            actual.push(component.as_os_str());
        }
        if matches {
            let mut path = source.clone();
            path.extend(requested);
            return filesystem.symlink_metadata(&path);
        }
    }
    filesystem.symlink_metadata(&kept.join(relative))
}

/// Whether a filesystem could resolve the two names to one entry, so that only the
/// filesystem can say whether they do: names differing only in ASCII letter case, or
/// either holding text whose case, normalization, short-name, or dot/space aliases this
/// does not decide.
fn may_alias(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> bool {
    match (left.to_str(), right.to_str()) {
        (Some(left), Some(right)) if left.is_ascii() && right.is_ascii() => {
            left.eq_ignore_ascii_case(right)
                || [left, right].iter().any(|name| {
                    name.contains('~') || name.starts_with(' ') || name.ends_with(['.', ' '])
                })
        }
        _ => true,
    }
}

/// Every entry of `merged`'s directory under `relative`, decided against the kept Order.
fn walk<F: FileSystem>(
    root: &Path,
    kept: &Path,
    merged: &Path,
    relative: &Path,
    moved: &mut Vec<(PathBuf, PathBuf)>,
    entries: &mut Vec<MergeEntry>,
    filesystem: &F,
) -> Result<(), Failure> {
    let failed = |path: PathBuf, error: &F::Error| {
        Failure::from(FailedOperation::filesystem(
            FailedPath::inside(root, path),
            error,
            filesystem,
        ))
    };
    let directory = merged.join(relative);
    let mut listed = filesystem
        .read_directory(&directory)
        .map_err(|error| failed(directory.clone(), &error))?;
    listed.sort_by(|left, right| left.name.cmp(&right.name));
    let metadata = if relative.as_os_str().is_empty() {
        let path = merged.join(".spielorders");
        let name = filesystem
            .entry_name(&path)
            .map_err(|error| failed(path.clone(), &error))?
            .filter(|name| {
                listed.iter().any(|entry| {
                    entry.name == *name && entry.kind == FileSystemEntryKind::Directory
                })
            })
            .ok_or_else(|| Failure::PreconditionsChanged {
                location: located_in_package(root, path),
            })?;
        Some(name)
    } else {
        None
    };
    for entry in listed {
        if metadata.as_ref() == Some(&entry.name) {
            continue;
        }
        let entry_relative = relative.join(&entry.name);
        let from = merged.join(&entry_relative);
        let to = kept.join(&entry_relative);
        let is_directory = entry.kind == FileSystemEntryKind::Directory;
        match kind_at(kept, &entry_relative, moved, filesystem)
            .map_err(|error| failed(to.clone(), &error))?
        {
            None => {
                moved.push((entry_relative, from.clone()));
                entries.push(MergeEntry {
                    action: EntryAction::Move,
                    from,
                    to,
                    directory: is_directory,
                });
            }
            Some(FileSystemEntryKind::Directory) if is_directory => {
                walk(
                    root,
                    kept,
                    merged,
                    &entry_relative,
                    moved,
                    entries,
                    filesystem,
                )?;
            }
            Some(_) => entries.push(MergeEntry {
                action: EntryAction::Discard,
                from,
                to,
                directory: is_directory,
            }),
        }
    }
    Ok(())
}

/// The kept record and the directory entries merging `orders` in `package` produces.
fn merged<F: FileSystem>(
    package: &CurrentPackageCandidate,
    orders: &[PathBuf],
    filesystem: &F,
) -> Result<(OrderDocument, Vec<OrderDocument>, Vec<MergeEntry>), Failure> {
    let found = resolved(package, orders)?;
    let root = &package.context.root.absolute_path;
    let kept = found[0];
    let mut candidate = kept.document.clone();
    let mut moved = Vec::new();
    let mut entries = Vec::new();
    for order in &found[1..] {
        fill_order(&mut candidate, &order.document);
        walk(
            root,
            &kept.paths.directory.absolute_path,
            &order.paths.directory.absolute_path,
            Path::new(""),
            &mut moved,
            &mut entries,
            filesystem,
        )?;
    }
    let candidate = candidate
        .validate()
        .map_err(|_| Failure::InvalidFacts)?
        .order()
        .clone();
    Ok((
        candidate,
        found[1..]
            .iter()
            .map(|order| order.document.clone())
            .collect(),
        entries,
    ))
}

pub(crate) fn plan_order_merge<F: FileSystem>(
    orders: &[PathBuf],
    filesystem: &F,
) -> Answer<OrderMergePlan> {
    Answer::of(|| {
        let kept = orders.first().ok_or(Failure::InvalidFacts)?;
        let (package, _) = crate::order_edits::resolve_order(kept, filesystem)?;
        // Every merged Order is a record the merge rewrites or removes.
        let changed = Touched::orders(orders.iter().cloned()).with_vendors(true);
        let package =
            load_package_to_change(&package.context.root.absolute_path, &changed, filesystem)?;
        let (candidate, merged_documents, entries) = merged(&package, orders, filesystem)?;
        let root = package.context.root.absolute_path.clone();
        let kept = resolved(&package, orders)?[0];
        let located = |path: &Path| located_in_package(&root, path.to_path_buf());
        let mut operations = Vec::new();
        if candidate != kept.document {
            operations.push(PlannedOperation::new(
                OperationKind::ReplaceOrderRecord,
                kept.paths.order.clone(),
            ));
        }
        for entry in &entries {
            let kind = match entry.action {
                EntryAction::Move => OperationKind::MoveMergedEntry,
                EntryAction::Discard => OperationKind::DiscardMergedEntry,
            };
            let mut operation = PlannedOperation::new(kind, located(&entry.from));
            operation.directory = entry.directory;
            operations.push(operation);
        }
        for directory in &orders[1..] {
            operations.push(PlannedOperation::new(
                OperationKind::MoveOrderDirectory,
                located(directory),
            ));
        }
        operations.push(PlannedOperation::new(
            OperationKind::RefreshPackageIndex,
            package.context.index.clone(),
        ));
        Ok(OrderMergePlan {
            kept: kept.paths.directory.clone(),
            merged: orders[1..].to_vec(),
            prior: kept.document.clone(),
            merged_documents,
            candidate,
            entries,
            operations,
            root,
        })
    })
}

pub(crate) fn commit_order_merge<E: Effects>(
    plan: OrderMergePlan,
    effects: &E,
) -> Answer<MutationReport<OrderMergeResult>> {
    let filesystem = effects.file_system();
    Answer::of(|| {
        let changed = |path: &Path| Failure::PreconditionsChanged {
            location: located_in_package(&plan.root, path.to_path_buf()),
        };
        let orders = std::iter::once(plan.kept.absolute_path.clone())
            .chain(plan.merged.iter().cloned())
            .collect::<Vec<_>>();
        let records = Touched::orders(orders.iter().cloned()).with_vendors(true);
        let current = load_package_to_change(&plan.root, &records, filesystem)?;
        // The merge planned again against the Package as it now stands must be the merge
        // planned: the same records and the same entries, decided the same way.
        let replanned =
            merged(&current, &orders, filesystem).map_err(|_| changed(&plan.kept.absolute_path))?;
        let kept = resolved(&current, &orders)?[0];
        if kept.document != plan.prior
            || replanned
                != (
                    plan.candidate.clone(),
                    plan.merged_documents.clone(),
                    plan.entries.clone(),
                )
        {
            return Err(changed(&plan.kept.absolute_path));
        }

        let record = kept.paths.order.absolute_path.clone();
        let candidate = plan
            .candidate
            .to_json_bytes()
            .map_err(|_| Failure::InvalidFacts)?;
        let prior_bytes = filesystem.read(&record).map_err(|_| changed(&record))?;
        if plan.candidate != plan.prior {
            let still_holds_the_prior = |bytes: &[u8]| {
                OrderDocument::from_json_bytes(bytes)
                    .ok()
                    .and_then(|document| document.validate().ok())
                    .is_some_and(|document| document.order() == &plan.prior)
            };
            replace_validated_file_expecting(
                &record,
                &candidate,
                DestinationExpectation::Equivalent(&still_holds_the_prior),
                filesystem,
                |_| Ok::<(), Never>(()),
            )
            .map_err(|error| Failure::of_replacement(&plan.root, &record, error))?;
        }

        let mut done: Vec<&MergeEntry> = Vec::new();
        for entry in plan
            .entries
            .iter()
            .filter(|entry| entry.action == EntryAction::Move)
        {
            if let Err(failure) = move_entry(&plan.root, &entry.from, &entry.to, filesystem) {
                // Everything this merge moved goes back, newest first, and the kept record
                // returns to what it held; whatever cannot be restored is reported beside
                // the failure that stopped the merge.
                let mut left = Vec::new();
                for moved in done.iter().rev() {
                    if move_entry(&plan.root, &moved.to, &moved.from, filesystem).is_err() {
                        left.push(crate::mutation::CleanupFailure::new(located_in_package(
                            &plan.root,
                            moved.to.clone(),
                        )));
                    }
                }
                if plan.candidate != plan.prior
                    && replace_validated_file_expecting(
                        &record,
                        &prior_bytes,
                        DestinationExpectation::Exactly(&candidate),
                        filesystem,
                        |_| Ok::<(), Never>(()),
                    )
                    .is_err()
                {
                    left.push(crate::mutation::CleanupFailure::new(located_in_package(
                        &plan.root,
                        record.clone(),
                    )));
                }
                return Err(if left.is_empty() {
                    failure
                } else {
                    Failure::CommitFailedCleanupIncomplete {
                        failure: Box::new(crate::mutation::CommitFailure::new(located_in_package(
                            &plan.root,
                            entry.from.clone(),
                        ))),
                        cleanup_failures: left,
                    }
                });
            }
            done.push(entry);
        }

        // What remains of each merged Order — its record and every discarded entry — is
        // removed as an Order removal removes one, so it stays recoverable.
        let mut warnings = Vec::new();
        for directory in &plan.merged {
            let removal = plan_order_removal(directory, filesystem)
                .result
                .and_then(|removal| commit_order_removal(removal, effects).result);
            match removal {
                Ok(report) => {
                    if let MutationReport::CommittedWithWarning { warnings: own, .. } = report {
                        warnings.extend(own);
                    }
                }
                Err(failure) => warnings.push(Warning::OrderNotRemoved(failure)),
            }
        }
        let refresh = IndexRefresh::after_commit(
            &plan.root,
            &Touched::orders(orders.iter().cloned()),
            filesystem,
        );
        warnings.extend(refresh.warning());
        Ok(MutationReport::committed(
            OrderMergeResult {
                directory: plan.kept.clone(),
            },
            warnings,
        ))
    })
}

/// Moves one entry to a path where nothing stands, never replacing what appeared there.
fn move_entry<F: FileSystem>(
    root: &Path,
    from: &Path,
    to: &Path,
    filesystem: &F,
) -> Result<(), Failure> {
    let failed = |path: &Path, error: &F::Error| {
        Failure::from(FailedOperation::filesystem(
            FailedPath::inside(root, path.to_path_buf()),
            error,
            filesystem,
        ))
    };
    let occupied = || Failure::PreconditionsChanged {
        location: located_in_package(root, to.to_path_buf()),
    };
    match filesystem
        .rename_new(from, to)
        .map_err(|error| failed(from, &error))?
    {
        RenameOutcome::Renamed => Ok(()),
        RenameOutcome::DestinationExists => Err(occupied()),
    }
}
