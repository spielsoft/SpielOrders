//! Planning and safe in-place persistence for one existing Order Directory.

use crate::{
    Answer, Assignments, Failure,
    canonical::load_every_record,
    change::PlannedAct,
    filesystem::staging_path,
    integrity::Records,
    journal::{Expect, Journal, Subject, planned_location},
    metadata::OrderDocument,
    mutation::{MutationReport, OperationKind},
    operations::{FailedOperation, FailedPath},
    order_facts::{OrderAssignments, OrderFacts, OrderField, Prior, refusal_of},
    ports::{
        FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind, LocalDate,
    },
    readme::{Location, located_in_package, package_relative_path_text},
    values::{self, Replacement},
};
use std::path::{Path, PathBuf};

/// The set an adoption resolves, with every rule it broke on its own among its faults;
/// planning reports them beside the Vendor, value, and Item faults, as a creation's are.
///
/// Adjudicated against the same one list of assignable fields as a creation — see
/// [`crate::order_facts`]. Title, Date Ordered, and Status are required, all three asked
/// for at once; nothing parses the candidate's name or contents to propose them.
pub(crate) fn adoption_set(assignments: &Assignments) -> OrderAssignments {
    OrderAssignments::new_order(
        assignments,
        &[
            OrderField::Title,
            OrderField::DateOrdered,
            OrderField::Status,
        ],
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderAdoptionPlan {
    root: PathBuf,
    directory: Location,
    pub(crate) facts: OrderFacts,
    candidate_observation: FileChangeObservation,
    readme_kind: Option<FileSystemEntryKind>,
    pub(crate) journal: Journal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderAdoptionResult {
    /// The adopted Order's Directory.
    pub directory: Location,
}

pub(crate) type OrderAdoptionReport = MutationReport<OrderAdoptionResult>;

pub(crate) fn plan_order_adoption<F: FileSystem>(
    selected_root: &Path,
    selected_directory: &Path,
    set: &OrderAssignments,
    items: &[Assignments],
    today: &dyn Fn() -> LocalDate,
    filesystem: &F,
) -> Answer<OrderAdoptionPlan> {
    Answer::of(|| {
        let package = load_every_record(selected_root, filesystem)?;
        let root_location = package.context.root.clone();
        let root = root_location.absolute_path.clone();
        let orders = package
            .context
            .orders_directory
            .as_ref()
            .expect("a current Package has an Orders Directory");
        if selected_directory
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            return Err(Failure::HiddenCandidate {
                path: selected_directory.to_path_buf(),
            });
        }
        match observe(&root_location, selected_directory, filesystem)? {
            Some(FileSystemEntryKind::Directory) => {}
            kind => {
                return Err(Failure::IneligibleCandidate {
                    path: selected_directory.to_path_buf(),
                    kind,
                });
            }
        }
        let directory = filesystem
            .canonicalize(selected_directory)
            .map_err(|error| {
                // Resolving the caller's own selected path is the operation that failed, so
                // nothing here located it and no Package spells it.
                Failure::from(
                    FailedOperation::filesystem(
                        FailedPath::of(selected_directory),
                        &error,
                        filesystem,
                    )
                    .under(&root_location),
                )
            })?;
        if directory.parent() != Some(&orders.absolute_path) {
            return Err(Failure::NotDirectChild {
                location: located_in_package(&root, directory),
            });
        }
        // Adoption is the one capability that names an Order Directory instead of
        // discovering it, so the classification that protects every reader does not
        // protect this. Asking the same question here keeps a directory the Package
        // declines to index from being adopted into it.
        if package_relative_path_text(&root, &directory).is_none() {
            return Err(Failure::NameUnusableCandidate {
                location: located_in_package(&root, directory),
            });
        }
        let candidate_observation = filesystem
            .observe_file_change(&directory)
            .map_err(|error| observation_failed(&root_location, &directory, &error, filesystem))?;

        let metadata_directory = directory.join(".spielorders");
        if let Some(kind) = observe(&root_location, &metadata_directory, filesystem)? {
            let order_path = metadata_directory.join("order.json");
            let already_managed = kind == FileSystemEntryKind::Directory
                && observe(&root_location, &order_path, filesystem)?
                    == Some(FileSystemEntryKind::File)
                && filesystem
                    .read(&order_path)
                    .ok()
                    .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
                    .is_some();
            return Err(if already_managed {
                Failure::AlreadyAnOrder {
                    location: planned_location(&root, order_path),
                }
            } else {
                Failure::ConflictingReservedState {
                    location: planned_location(&root, metadata_directory),
                }
            });
        }

        let readme_path = directory.join("README.md");
        let readme_kind = observe(&root_location, &readme_path, filesystem)?;
        let order_staging = stage_of(&metadata_directory.join("order.json"));
        let readme_staging = stage_of(&readme_path);
        for path in [&order_staging, &readme_staging] {
            if observe(&root_location, path, filesystem)?.is_some() {
                return Err(Failure::StagingCollision {
                    location: planned_location(&root, path.clone()),
                });
            }
        }

        // The Name selects; the record stores what it selected, as a creation's does.
        let facts =
            OrderFacts::resolve(&package, Prior::New { today }, set, items).map_err(refusal_of)?;

        let planned_values = (!facts.additions.is_empty()).then(|| Replacement::planned(&package));
        // The record as planned, under the planning identity; the commit writes the one it
        // mints.
        let planned_order = facts.document.to_json_bytes().unwrap_or_default();
        let journal = adoption_journal(
            &root,
            &directory,
            readme_kind.is_none(),
            package.context.index.clone(),
            planned_order,
            planned_values,
        );

        Ok(OrderAdoptionPlan {
            directory: planned_location(&root, directory),
            root,
            facts,
            candidate_observation,
            readme_kind,
            journal,
        })
    })
}

/// The commit adopting the directory at `directory`: its metadata directory created, its
/// record staged with `order` and a README staged where `create_readme`, the values it
/// introduces kept, and the record then published by one link. The stages are removed
/// after that, as tidying up.
fn adoption_journal(
    root: &Path,
    directory: &Path,
    create_readme: bool,
    index: Location,
    order: Vec<u8>,
    values: Option<Replacement>,
) -> Journal {
    let managed = PlannedAct::Managed;
    let metadata = directory.join(".spielorders");
    let record = metadata.join("order.json");
    let readme = directory.join("README.md");
    let mut journal = Journal::new(root);
    journal.create_directory(metadata, managed(OperationKind::CreateMetadataDirectory));
    journal.stage(
        record.clone(),
        order,
        managed(OperationKind::StageOrderRecord),
    );
    if create_readme {
        journal.stage(
            readme.clone(),
            Vec::new(),
            managed(OperationKind::StageReadme),
        );
        journal.link(
            readme.clone(),
            managed(OperationKind::CommitReadme),
            Subject::Other,
        );
    }
    // The values the Order introduces are kept before its record is published.
    if let Some(values) = values {
        journal.replace(
            values.path,
            values.candidate,
            Expect::Bytes(values.prior),
            managed(OperationKind::ReplaceVendorRecord),
            Subject::Vendors,
        );
    }
    journal.link(
        record.clone(),
        managed(OperationKind::CommitOrderRecord),
        Subject::Order(directory.to_path_buf()),
    );
    journal.commit_point();
    if create_readme {
        journal.remove_stage(readme, managed(OperationKind::RemoveStaging));
    }
    journal.remove_stage(record, managed(OperationKind::RemoveStaging));
    journal.refreshing(index);
    journal
}

pub(crate) fn commit_order_adoption<F: FileSystem>(
    plan: OrderAdoptionPlan,
    new_uuid: &dyn Fn() -> String,
    filesystem: &F,
) -> Answer<OrderAdoptionReport> {
    Answer::of(|| {
        let current = load_every_record(&plan.root, filesystem)?;
        let metadata_directory = plan.directory.absolute_path.join(".spielorders");
        let readme_path = plan.directory.absolute_path.join("README.md");
        let readme_staging = stage_of(&readme_path);
        match filesystem.observe_file_change(&plan.directory.absolute_path) {
            Ok(current) if observations_prove_same(plan.candidate_observation, current) => {}
            Ok(_) => {
                let path = plan.directory.absolute_path.clone();
                return Err(preconditions_changed(&plan, path));
            }
            Err(error) => {
                let failed = FailedOperation::inside(
                    &plan.root,
                    plan.directory.absolute_path.clone(),
                    &error,
                    filesystem,
                );
                return Err(failed.into());
            }
        }
        for (path, expected) in [
            (
                plan.directory.absolute_path.clone(),
                Some(FileSystemEntryKind::Directory),
            ),
            (metadata_directory.clone(), None),
            (readme_path.clone(), plan.readme_kind),
            (readme_staging.clone(), None),
        ] {
            match filesystem.symlink_metadata(&path) {
                Ok(observed) if observed == expected => {}
                Ok(_) => return Err(preconditions_changed(&plan, path.to_path_buf())),
                Err(error) => {
                    let failed =
                        FailedOperation::inside(&plan.root, path.to_path_buf(), &error, filesystem);
                    return Err(failed.into());
                }
            }
        }
        // The Vendor, the reusable values, and the Orders Directory the facts were resolved
        // against, re-observed as a creation re-observes them.
        plan.facts.confirm(&current)?;

        let generated = new_uuid();
        let order = match plan.facts.with_identity(&generated) {
            Some(order) => order,
            None => {
                return Err(Failure::InvalidGeneratedIdentity { generated });
            }
        };
        if Records::of(&current)
            .holder(&order.order_id, None)
            .is_some()
        {
            return Err(Failure::GeneratedIdentityCollision {
                order_id: generated,
            });
        }
        let candidate = order
            .to_json_bytes()
            .expect("a validated newest-version Order serializes deterministically");
        let values = values::replacement(&current, &plan.facts.additions, filesystem)?;
        let committed = adoption_journal(
            &plan.root,
            &plan.directory.absolute_path,
            plan.readme_kind.is_none(),
            current.context.index.clone(),
            candidate,
            values,
        )
        .run(filesystem)?;
        let mut refresh = committed.refresh(&plan.root, filesystem);
        // A refreshed Package that no longer carries the adopted Order cannot confirm it,
        // and the commit says so instead of silently claiming a confirmation it did not get.
        let confirmed = refresh.package().is_some_and(|package| {
            package
                .healthy_orders
                .iter()
                .any(|candidate| candidate.document.order_id == order.order_id)
        });
        if !confirmed {
            refresh.unconfirmed();
        }
        let result = OrderAdoptionResult {
            directory: plan.directory.clone(),
        };
        Ok(MutationReport::committed(
            result,
            committed.warnings(refresh),
        ))
    })
}

/// One observation, with the Root this adoption resolved so a failure can name where it
/// happened in the Package's own spelling. The Root is carried rather than derived because
/// the first observation is of the caller's own selected path, which may be outside the
/// Package entirely — and `FailedPath::inside` is what answers both cases.
fn observe<F: FileSystem>(
    root: &Location,
    path: &Path,
    filesystem: &F,
) -> Result<Option<FileSystemEntryKind>, Failure> {
    filesystem
        .symlink_metadata(path)
        .map_err(|error| observation_failed(root, path, &error, filesystem))
}

fn observation_failed<F: FileSystem>(
    root: &Location,
    path: &Path,
    error: &F::Error,
    filesystem: &F,
) -> Failure {
    Failure::from(
        FailedOperation::filesystem(
            FailedPath::inside(&root.absolute_path, path.to_path_buf()),
            error,
            filesystem,
        )
        .under(root),
    )
}

fn observations_prove_same(planned: FileChangeObservation, current: FileChangeObservation) -> bool {
    if planned.kind != Some(FileSystemEntryKind::Directory)
        || current.kind != Some(FileSystemEntryKind::Directory)
    {
        return false;
    }
    matches!(
        (planned.evidence, current.evidence),
        (
            FileChangeEvidence::Comparable {
                comparison_domain: planned_domain,
                sequence: planned_sequence,
            },
            FileChangeEvidence::Comparable {
                comparison_domain: current_domain,
                sequence: current_sequence,
            },
        ) if planned_domain == current_domain && planned_sequence == current_sequence
    )
}

/// The stage beside the managed file at `path`.
fn stage_of(path: &Path) -> PathBuf {
    staging_path(path).expect("a managed file has a basename")
}

/// One path this plan named inside its own Package, as that Package locates it.
fn preconditions_changed(plan: &OrderAdoptionPlan, path: PathBuf) -> Failure {
    Failure::PreconditionsChanged {
        location: planned_location(&plan.root, path),
    }
}
