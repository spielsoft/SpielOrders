//! Merging Orders in precedence order: the first Order named is kept, and each later one
//! only fills what the result still lacks — its facts, its Items, and the entries of its
//! directory — before what remains of it is removed as an Order removal removes one.
//!
//! Decision sources: Define the Order record and lifecycle contract; Define the SpielOrders
//! package topology; Durable managed-file writes.

use crate::{
    Answer, Failure, Warning,
    canonical::{CurrentPackageCandidate, HealthyOrder, Touched, load_package_to_change},
    change::PlannedAct,
    journal::{Journal, ManagedRecord, Subject},
    metadata::{OrderDocument, OrderItem, Vendor},
    mutation::{MutationReport, OperationKind},
    operations::{FailedOperation, FailedPath},
    ports::{Effects, FileSystem, FileSystemEntryKind},
    readme::{Location, located_in_package},
    removal::{self, plan_order_removal},
    values::same_optional,
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
    /// The Vendor the kept Order references before the merge, and after it.
    vendors: [Option<Vendor>; 2],
    entries: Vec<MergeEntry>,
    /// Every operation the merge performs, its merged Orders' removals among them.
    pub(crate) journal: Journal,
}

impl OrderMergePlan {
    /// The kept Order's record before the merge fills it, and after, each with the Vendor
    /// it references.
    pub(crate) fn records(&self) -> [(&OrderDocument, Option<&Vendor>); 2] {
        [
            (&self.prior, self.vendors[0].as_ref()),
            (&self.candidate, self.vendors[1].as_ref()),
        ]
    }
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
            .ok_or_else(|| Failure::OrderNotFound {
                directory: crate::operations::FailedPath::inside(
                    &package.context.root.absolute_path,
                    directory.clone(),
                ),
                root: Some(package.context.root.absolute_path.clone()),
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

/// `kept` with each fact it lacks taken from `merged`, and each of `merged`'s Items that
/// the kept Order did not hold before the merge (`original`) added after its own. An Order
/// Group comes only with its Project, or to an Order in the same Project; an Order gaining
/// a Project takes that Project's group with it, as an edit changing the Project would
/// clear its own. Refused where `merged` would add Items priced in another currency.
fn fill_order(
    kept: &mut OrderDocument,
    original: &[OrderItem],
    merged: &OrderDocument,
    location: &Location,
) -> Result<(), Failure> {
    fn fill<T: Clone>(kept: &mut Option<T>, merged: &Option<T>) {
        if kept.is_none() {
            kept.clone_from(merged);
        }
    }
    let added = merged
        .items
        .iter()
        .filter(|item| !original.contains(item))
        .cloned()
        .collect::<Vec<_>>();
    // Prices are never relabeled, and having no currency is a currency of its own: Items join
    // only Items priced in the same currency. A result with no Items has no prices, so it
    // takes the currency of the first Order that brings some, unless it states another; an
    // Order that brings no Items changes nothing about currency.
    if !added.is_empty() {
        let priced = !kept.items.is_empty() || kept.currency.is_some();
        if priced && kept.currency != merged.currency {
            return Err(Failure::CurrencyDiffers {
                location: location.clone(),
            });
        }
        kept.currency.clone_from(&merged.currency);
    }
    fill(&mut kept.vendor_id, &merged.vendor_id);
    fill(&mut kept.orderer, &merged.orderer);
    fill(&mut kept.ordering_institution, &merged.ordering_institution);
    let same_project = same_optional(kept.project.as_deref(), merged.project.as_deref());
    if kept.project.is_none() && merged.project.is_some() {
        kept.project.clone_from(&merged.project);
        kept.order_group.clone_from(&merged.order_group);
    } else if same_project {
        fill(&mut kept.order_group, &merged.order_group);
    }
    kept.items.extend(added);
    Ok(())
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
        fill_order(
            &mut candidate,
            &kept.document.items,
            &order.document,
            &order.paths.directory,
        )?;
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
        // Every merged Order must be removable as `order remove` removes one, or the merge
        // would leave it active beside the Order that now holds its Items and files.
        let mut removals = Vec::new();
        for directory in &orders[1..] {
            removals.push(plan_order_removal(directory, filesystem).result?);
        }
        let mut journal = merge_journal(
            &root,
            kept,
            &orders[1..],
            &candidate,
            &entries,
            package.context.index.clone(),
        );
        // Each merged Order is removed as `order remove` removes it, so its removal's own
        // operations are the merge's, with one refresh for the whole merge and the
        // Removed Orders Directory created once.
        for removal in &removals {
            journal.listing_after(&removal.journal);
        }
        let vendor_of = |document: &OrderDocument| {
            let id = document.vendor_id.as_ref()?;
            package
                .vendors
                .iter()
                .find(|vendor| &vendor.vendor_id == id)
                .cloned()
        };
        let vendors = [vendor_of(&kept.document), vendor_of(&candidate)];
        Ok(OrderMergePlan {
            kept: kept.paths.directory.clone(),
            merged: orders[1..].to_vec(),
            prior: kept.document.clone(),
            merged_documents,
            vendors,
            candidate,
            entries,
            journal,
            root,
        })
    })
}

/// The commit filling `kept` with `candidate` and moving into it each entry of a merged
/// Order it does not hold; an entry it does hold is left for that Order's removal.
fn merge_journal(
    root: &Path,
    kept: &HealthyOrder,
    merged: &[PathBuf],
    candidate: &OrderDocument,
    entries: &[MergeEntry],
    index: Location,
) -> Journal {
    let managed = PlannedAct::Managed;
    let mut journal = Journal::new(root);
    if candidate != &kept.document {
        // The record the merge was planned against is the one it may replace: its meaning
        // is what is compared, so a record reformatted by hand is still filled.
        let record = ManagedRecord::order(
            kept.paths.order.absolute_path.clone(),
            kept.document.clone(),
        );
        journal.replace(
            record.path().to_path_buf(),
            candidate
                .to_json_bytes()
                .expect("a validated Order serializes deterministically"),
            record.expectation(),
            managed(OperationKind::ReplaceOrderRecord),
            Subject::Order(kept.paths.directory.absolute_path.clone()),
        );
    }
    for entry in entries {
        match entry.action {
            EntryAction::Move => journal.move_out(
                entry.from.clone(),
                entry.to.clone(),
                entry.directory,
                managed(OperationKind::MoveMergedEntry),
            ),
            EntryAction::Discard => journal.announce(
                entry.from.clone(),
                entry.directory,
                managed(OperationKind::DiscardMergedEntry),
            ),
        }
    }
    // Every merged Order loses the entries moved out of it, and the kept Order gains them.
    journal.touching(
        std::iter::once(kept.paths.directory.absolute_path.clone()).chain(merged.iter().cloned()),
    );
    journal.refreshing(index);
    journal
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

        if plan.candidate.to_json_bytes().is_err() {
            return Err(Failure::InvalidFacts);
        }
        // Moving every entry that moves, after filling the kept record: a failure undoes
        // what the merge did and names what it could not put back.
        let mut committed = merge_journal(
            &plan.root,
            kept,
            &plan.merged,
            &plan.candidate,
            &plan.entries,
            current.context.index.clone(),
        )
        .run(filesystem)?;

        // What remains of each merged Order — its record and every discarded entry — is
        // removed as an Order removal removes one, so it stays recoverable. The merge has
        // committed by now, so a removal that fails is a warning of it.
        for directory in &plan.merged {
            let removal = plan_order_removal(directory, filesystem)
                .result
                .and_then(|removal| removal::confirmed(&removal, filesystem))
                .and_then(|journal| journal.run(filesystem));
            match removal {
                Ok(removed) => committed.absorb(removed),
                Err(failure) => committed.warn(Warning::OrderNotRemoved(failure)),
            }
        }
        // One refresh for the whole merge, reading again every record it changed.
        let refresh = committed.refresh(&plan.root, filesystem);
        Ok(MutationReport::committed(
            OrderMergeResult {
                directory: plan.kept.clone(),
            },
            committed.warnings(refresh),
        ))
    })
}
