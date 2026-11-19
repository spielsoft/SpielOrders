//! Planning and persistence for creating one newest-version Order.

use crate::{
    Answer, Assignments, Failure,
    canonical::{CurrentPackageCandidate, Touched, load_package_to_change},
    change::PlannedAct,
    integrity::Records,
    journal::{Expect, Journal, Subject, planned_location},
    metadata::OrderDocument,
    mutation::{MutationReport, OperationKind},
    operations::{FailedOperation, FailedPath},
    order_facts::{OrderAssignments, OrderFacts, OrderField, Prior, refusal_of},
    ports::{FileSystem, LocalDate},
    readme::Location,
    values::{self, Replacement},
};
use std::path::{Path, PathBuf};

/// The set a creation resolves, or every rule it broke on its own.
///
/// **Adjudicated here rather than by the caller**, because which fields an Order has and
/// which of them may be assigned are the Package's own rules — see
/// [`crate::order_facts`]. A caller supplying a `field=value` language therefore refuses
/// nothing itself.
///
/// A creation's one required fact is the Order Title. Every other fact takes its
/// established default, and an empty value is simply an absent fact. Planning refuses
/// every rule the facts, their Vendor and values, and the Items broke, together.
pub(crate) fn creation_set(assignments: &Assignments) -> OrderAssignments {
    OrderAssignments::new_order(assignments, &[OrderField::Title])
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderCreationPlan {
    root: PathBuf,
    directory: Location,
    staging_directory: Location,
    pub(crate) facts: OrderFacts,
    pub(crate) journal: Journal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderCreationResult {
    /// The created Order's Directory.
    pub directory: Location,
}

pub(crate) type OrderCreationReport = MutationReport<OrderCreationResult>;

pub(crate) fn plan_order_creation<F: FileSystem>(
    selected_root: &Path,
    set: &OrderAssignments,
    rows: &[Assignments],
    today: &dyn Fn() -> LocalDate,
    filesystem: &F,
) -> Answer<OrderCreationPlan> {
    Answer::of(|| {
        let package = load_package_to_change(
            selected_root,
            &Touched::default().with_vendors(true),
            filesystem,
        )?;
        let root = package.context.root.absolute_path.clone();
        let orders = package
            .context
            .orders_directory
            .as_ref()
            .expect("a current Package has an Orders Directory");
        // The Name selects; the record stores what it selected, resolved as every Vendor
        // Name is, so a Vendor reached here is the one `vendor show` would report.
        let facts =
            OrderFacts::resolve(&package, Prior::New { today }, set, rows).map_err(refusal_of)?;
        let order = &facts.document;
        let title = order.title.as_str().to_owned();
        let title_component = normalize_filename_component(&title);
        if title_component.is_empty() {
            return Err(Failure::EmptyProjectedComponent);
        }
        let basename = projected_order_basename(
            order.date_ordered.as_str(),
            facts.vendor.as_ref().map(|vendor| vendor.name.as_str()),
            &title_component,
            order.orderer.as_deref(),
        );
        let directory = orders.absolute_path.join(&basename);
        let staging_directory = orders
            .absolute_path
            .join(format!(".{basename}.spielorders-stage"));
        for path in [&directory, &staging_directory] {
            match filesystem.symlink_metadata(path) {
                Ok(None) => {}
                Ok(found @ Some(_)) => {
                    return Err(Failure::DestinationCollision {
                        root: package.context.root.clone(),
                        location: planned_location(&root, path.to_path_buf()),
                        found,
                    });
                }
                Err(error) => {
                    return Err(Failure::from(
                        FailedOperation::filesystem(
                            FailedPath::inside(&root, path.to_path_buf()),
                            &error,
                            filesystem,
                        )
                        .under(&package.context.root),
                    ));
                }
            }
        }
        // The values the Order introduces are kept before its directory is committed.
        let planned_values = (!facts.additions.is_empty()).then(|| Replacement::planned(&package));
        // The record as planned, under the planning identity; the commit writes the one it
        // mints.
        let planned_order = facts.document.to_json_bytes().unwrap_or_default();
        let journal = creation_journal(
            &root,
            &staging_directory,
            &directory,
            package.context.index.clone(),
            planned_order,
            planned_values,
        );

        Ok(OrderCreationPlan {
            directory: planned_location(&root, directory),
            staging_directory: planned_location(&root, staging_directory),
            root,
            facts,
            journal,
        })
    })
}

/// The commit creating one Order: its directory assembled at `staging` with `order` as its
/// record, the values it introduces kept, and the whole directory then published at
/// `directory` by one move.
fn creation_journal(
    root: &Path,
    staging: &Path,
    directory: &Path,
    index: Location,
    order: Vec<u8>,
    values: Option<Replacement>,
) -> Journal {
    let managed = PlannedAct::Managed;
    let metadata = staging.join(".spielorders");
    let mut journal = Journal::new(root);
    journal.create_directory(
        staging.to_path_buf(),
        managed(OperationKind::CreateStagingDirectory),
    );
    journal.create_directory(
        metadata.clone(),
        managed(OperationKind::CreateMetadataDirectory),
    );
    journal.write_unpublished(
        metadata.join("order.json"),
        order,
        managed(OperationKind::WriteOrderRecord),
    );
    journal.write_unpublished(
        staging.join("README.md"),
        Vec::new(),
        managed(OperationKind::WriteReadme),
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
    journal.move_entry(
        staging.to_path_buf(),
        directory.to_path_buf(),
        managed(OperationKind::CommitDirectory),
        Subject::Order(directory.to_path_buf()),
    );
    journal.refreshing(index);
    journal
}

pub(crate) fn commit_order_creation<F: FileSystem>(
    plan: OrderCreationPlan,
    new_uuid: &dyn Fn() -> String,
    filesystem: &F,
) -> Answer<OrderCreationReport> {
    Answer::of(|| {
        let current = load_package_to_change(
            &plan.root,
            &Touched::default().with_vendors(true),
            filesystem,
        )?;
        // Before the destination, because a destination projected from an Orders Directory
        // this Package no longer configures is not the path whose absence means anything.
        plan.facts.confirm(&current)?;
        for path in [
            plan.directory.absolute_path.clone(),
            plan.staging_directory.absolute_path.clone(),
        ] {
            match filesystem.symlink_metadata(&path) {
                Ok(None) => {}
                Ok(Some(_)) | Err(_) => {
                    let location = planned_location(&plan.root, path.to_path_buf());
                    return Err(Failure::PreconditionsChanged { location });
                }
            }
        }

        let generated = new_uuid();
        let order = match plan.facts.with_identity(&generated) {
            Some(order) => order,
            None => {
                return Err(Failure::InvalidGeneratedIdentity { generated });
            }
        };
        if Records::of(&current).holder(&generated, None).is_some() {
            return Err(Failure::GeneratedIdentityCollision {
                order_id: generated,
            });
        }
        let candidate = order
            .to_json_bytes()
            .expect("a validated newest-version Order serializes deterministically");
        let values = values::replacement(&current, &plan.facts.additions, filesystem)?;
        let committed = creation_journal(
            &plan.root,
            &plan.staging_directory.absolute_path,
            &plan.directory.absolute_path,
            current.context.index.clone(),
            candidate,
            values,
        )
        .run(filesystem)?;
        let mut refresh = committed.refresh(&plan.root, filesystem);
        // A concurrent external change can remove or damage the Order between the commit
        // and the refresh; the creation still happened, and the refresh cannot confirm it.
        if refresh
            .package()
            .is_some_and(|package| !holds(package, &order))
        {
            refresh.unconfirmed();
        }
        Ok(MutationReport::committed(
            OrderCreationResult {
                directory: plan.directory.clone(),
            },
            refresh.warning().into_iter().collect(),
        ))
    })
}

/// Whether the refreshed `package` holds the committed `order`.
fn holds(package: &CurrentPackageCandidate, order: &OrderDocument) -> bool {
    package
        .healthy_orders
        .iter()
        .any(|candidate| candidate.document.order_id == order.order_id)
}

/// The one decided aligned Order Directory basename,
/// `YYYY_MM_DD[_Vendor]_Title[_Orderer]`. The Vendor and Orderer components are
/// omitted when absent rather than emitted empty. Creation and explicit alignment
/// share this projection so the two can never disagree about a directory's name.
pub(crate) fn projected_order_basename(
    date_ordered: &str,
    vendor_name: Option<&str>,
    title_component: &str,
    orderer: Option<&str>,
) -> String {
    let mut components = vec![date_ordered.replace('-', "_")];
    if let Some(vendor_name) = vendor_name {
        components.push(normalize_filename_component(vendor_name));
    }
    components.push(title_component.to_owned());
    if let Some(orderer) = orderer {
        components.push(normalize_filename_component(orderer));
    }
    components.join("_")
}

pub(crate) fn normalize_filename_component(value: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_underscore = false;
    for character in value.trim().chars() {
        let replace = character.is_whitespace()
            || character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            );
        if replace || character == '_' {
            if !last_was_underscore {
                normalized.push('_');
                last_was_underscore = true;
            }
        } else {
            normalized.push(character);
            last_was_underscore = false;
        }
    }
    normalized
}
