//! Planning and persistence for one existing Order's own record: an editor's whole draft
//! of its facts and its ordered Item list, resolved by [`crate::order_facts`] over the
//! stored record.

use crate::Assignments;
use crate::{
    Answer, Failure,
    answer::DraftFailure,
    canonical::{CurrentPackageCandidate, HealthyOrder, Touched, load_package_to_change},
    change::PlannedAct,
    journal::{Expect, Journal, ManagedRecord, Subject},
    metadata::{OrderDocument, StoredValues},
    mutation::{MutationReport, OperationKind},
    order_facts::{OrderAssignments, OrderFacts, Prior},
    ports::{FileSystem, LocalDate},
    readme::{Location, located_in_package},
    values::Replacement,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct OrderEditPlan {
    root: PathBuf,
    directory: Location,
    order_path: PathBuf,
    pub(crate) facts: OrderFacts,
    prior_document: OrderDocument,
    pub(crate) journal: Journal,
}

impl OrderEditPlan {
    /// Bind an editor's plan to the original document its draft was based on.
    /// Commit adjudicates this expectation against the current canonical document.
    pub(crate) fn require_original(mut self, original: Option<&OrderDocument>) -> Self {
        if let Some(original) = original {
            self.prior_document = original.clone();
        }
        self
    }

    /// The new values this edit adds to those the Package keeps.
    pub(crate) fn additions(&self) -> &StoredValues {
        &self.facts.additions
    }

    /// Whether committing this plan would change the canonical Order record.
    pub fn changes_canonical_order(&self) -> bool {
        self.facts.document != self.prior_document
    }
}

/// Plans an editor's whole draft of one Order: its facts and its complete Item list.
pub(crate) fn plan_order_edit<F: FileSystem>(
    order_directory: &Path,
    assignments: &Assignments,
    items: &[Assignments],
    today: &dyn Fn() -> LocalDate,
    filesystem: &F,
) -> Answer<OrderEditPlan> {
    Answer::of(|| {
        let (package, order) = resolve_order(order_directory, filesystem)?;
        edited_order(&package, &order, assignments, items, today).map_err(Failure::InvalidDraft)
    })
}

/// The plan that rewrites `order` with `assignments` over its stored facts and `items` in
/// place of its stored Items; or every rule that broke, the facts' in adjudication order
/// and then each row's.
pub(crate) fn edited_order(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
    assignments: &Assignments,
    items: &[Assignments],
    today: &dyn Fn() -> LocalDate,
) -> Result<OrderEditPlan, Vec<DraftFailure>> {
    let prior = Prior::Stored {
        document: &order.document,
        today,
    };
    let facts = OrderFacts::resolve(package, prior, &OrderAssignments::of(assignments), items)?;
    // An unchanged record is committed by performing nothing, so its plan must not announce
    // managed operations its commit will never reach.
    let root = &package.context.root.absolute_path;
    let journal = if facts.document == order.document {
        Journal::new(root)
    } else {
        let record = ManagedRecord::order(
            order.paths.order.absolute_path.clone(),
            order.document.clone(),
        );
        let values = (!facts.additions.is_empty()).then(|| Replacement::planned(package));
        edit_journal(
            root,
            &order.paths.directory.absolute_path,
            (&record, record.expectation()),
            facts.document.to_json_bytes().unwrap_or_default(),
            values,
            package.context.index.clone(),
        )
    };
    Ok(OrderEditPlan {
        root: package.context.root.absolute_path.clone(),
        directory: order.paths.directory.clone(),
        order_path: order.paths.order.absolute_path.clone(),
        facts,
        prior_document: order.document.clone(),
        journal,
    })
}

/// The commit replacing the Order record planned as `record` with `candidate`, expected to
/// still hold what `record` names, and the Vendor record keeping the values it introduces.
fn edit_journal(
    root: &Path,
    directory: &Path,
    (record, expect): (&ManagedRecord, Expect),
    candidate: Vec<u8>,
    values: Option<Replacement>,
    index: Location,
) -> Journal {
    let managed = PlannedAct::Managed;
    let mut journal = Journal::new(root);
    journal.replace(
        record.path().to_path_buf(),
        candidate,
        expect,
        managed(OperationKind::ReplaceOrderRecord),
        Subject::Order(directory.to_path_buf()),
    );
    if let Some(values) = values {
        journal.replace(
            values.path,
            values.candidate,
            Expect::Bytes(values.prior),
            managed(OperationKind::ReplaceVendorRecord),
            Subject::Vendors,
        );
    }
    journal.refreshing(index);
    journal
}

/// Resolves the Order Directory's own Package through its normalized locator, then
/// requires the directory to be one healthy Order of that Package.
pub(crate) fn resolve_order<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<(CurrentPackageCandidate, HealthyOrder), Failure> {
    let located = crate::order_reads::locate_order(order_directory, filesystem)?;
    let directory = &located.directory;
    // The Order this resolves is the record a change will rewrite, so it is read from
    // disk with the Vendor record it may reference, never taken from the index.
    let changed = Touched::orders([directory.clone()]).with_vendors(true);
    let package = located.package(load_package_to_change(&located.root, &changed, filesystem))?;
    if let Some(order) = package
        .healthy_orders
        .iter()
        .find(|candidate| &candidate.paths.directory.absolute_path == directory)
        .cloned()
    {
        return Ok((package, order));
    }
    if let Some(damaged) = package
        .damaged_orders
        .iter()
        .find(|candidate| &candidate.paths.directory.absolute_path == directory)
    {
        return Err(Failure::DamagedOrder {
            context: Box::new(package.context.clone()),
            diagnostics: package.order_diagnostics(&damaged.paths),
            paths: Box::new(damaged.paths.clone()),
            order_id: damaged.order_id.clone(),
        });
    }
    Err(located.not_held())
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderEditResult {
    /// The edited Order's Directory.
    pub directory: Location,
}

pub(crate) type OrderEditReport = MutationReport<OrderEditResult>;

pub(crate) fn commit_order_edit<F: FileSystem>(
    plan: OrderEditPlan,
    filesystem: &F,
) -> Answer<OrderEditReport> {
    Answer::of(|| {
        let changed = Touched::orders([plan.directory.absolute_path.clone()]).with_vendors(true);
        let current = load_package_to_change(&plan.root, &changed, filesystem)?;
        let Some(order) = current.healthy_orders.iter().find(|candidate| {
            candidate.paths.directory.absolute_path == plan.directory.absolute_path
        }) else {
            let path = plan.directory.absolute_path.clone();
            return Err(preconditions_changed(&plan, path));
        };
        if order.document != plan.prior_document {
            let path = plan.order_path.clone();
            return Err(preconditions_changed(&plan, path));
        }
        // The Vendor and the reusable values the facts were resolved against, re-observed as
        // a creation re-observes them.
        plan.facts.confirm(&current)?;

        if !plan.changes_canonical_order() {
            let result = OrderEditResult {
                directory: plan.directory.clone(),
            };
            return Ok(MutationReport::Unchanged { result });
        }
        let Ok(candidate) = plan.facts.document.to_json_bytes() else {
            let path = plan.order_path.clone();
            return Err(preconditions_changed(&plan, path));
        };
        // The Order revalidated above is the one this edit is authorized to replace, so a
        // writer landing between that revalidation and the rename is refused rather than
        // overwritten. Alone, its meaning is what is compared, not its bytes; replaced together
        // with the Vendor record, it is read once more here and must still mean what was
        // planned, and its replacement then expects exactly those bytes. Which rule to carry
        // where is in `docs/architecture/durable-writes.md`.
        let record = ManagedRecord::order(plan.order_path.clone(), plan.prior_document.clone());
        let values = crate::values::replacement(&current, &plan.facts.additions, filesystem)?;
        let expect = match &values {
            Some(_) => Expect::Bytes(record.confirm(&plan.root, filesystem)?),
            None => record.expectation(),
        };
        let committed = edit_journal(
            &plan.root,
            &plan.directory.absolute_path,
            (&record, expect),
            candidate,
            values,
            current.context.index.clone(),
        )
        .run(filesystem)?;
        let refresh = committed.refresh(&plan.root, filesystem);
        Ok(MutationReport::committed(
            OrderEditResult {
                directory: plan.directory.clone(),
            },
            committed.warnings(refresh),
        ))
    })
}

/// One path this plan named inside its own Package, as that Package locates it.
fn preconditions_changed(plan: &OrderEditPlan, path: PathBuf) -> Failure {
    Failure::PreconditionsChanged {
        location: located_in_package(&plan.root, path),
    }
}
