//! The values a Package keeps beside its Vendors: Ordering Institutions, Orderers,
//! Projects, and each Project's Order Groups.
//!
//! A Package's values are the ones `vendors.json` keeps and the ones its healthy Orders
//! use, so a Package whose record keeps none still offers every value its Orders use.
//! Values compare trimmed and case-insensitively, and a kept spelling is preferred over
//! an Order's.
//!
//! Decision sources: Define the Order record and lifecycle contract; Define durable
//! metadata schemas and version evolution.

use crate::{
    Failure,
    canonical::{CurrentPackageCandidate, ReusableValueProjection},
    filesystem::{
        DestinationExpectation, FileReplacement, replace_validated_file_expecting, staging_path,
    },
    metadata::{
        NonBlankString, OrderDocument, StoredProject, StoredValues, VendorsDocument, Version1,
    },
    mutation::{CleanupFailure, CommitFailure, Never},
    operations::FailedOperation,
    package::{PackageMetadata, ReusableValue, ReusableValues},
    ports::FileSystem,
    readme::{Location, located_in_package},
};
use std::{cmp::Ordering, collections::BTreeMap, path::PathBuf};

/// Whether two spellings are one value: trimmed, and without regard to letter case.
pub(crate) fn same_value(left: &str, right: &str) -> bool {
    focaccia::unicode_full_case_eq(left.trim(), right.trim())
}

/// Text ordered so that two spellings of one value are equal.
#[derive(Clone, Debug)]
struct ValueKey(String);

impl ValueKey {
    fn of(text: &str) -> Option<Self> {
        let text = text.trim();
        (!text.is_empty()).then(|| Self(text.to_owned()))
    }
}

impl Ord for ValueKey {
    fn cmp(&self, other: &Self) -> Ordering {
        focaccia::unicode_full_casecmp(&self.0, &other.0)
    }
}

impl PartialOrd for ValueKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ValueKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for ValueKey {}

/// Kept values trimmed, each once under its first spelling, in byte order; a Project
/// kept twice keeps the groups of both.
pub(crate) fn normalize(values: &mut StoredValues) {
    normalize_list(&mut values.ordering_institutions);
    normalize_list(&mut values.orderers);
    normalize_list(&mut values.order_groups);
    let mut projects = Vec::<StoredProject>::new();
    for mut project in values.projects.drain(..) {
        project.name.normalize_single_line();
        match projects
            .iter_mut()
            .find(|kept| same_value(kept.name.as_str(), project.name.as_str()))
        {
            Some(kept) => kept.order_groups.extend(project.order_groups),
            None => projects.push(project),
        }
    }
    for project in &mut projects {
        normalize_list(&mut project.order_groups);
    }
    projects.sort_by(|left, right| {
        left.name
            .as_str()
            .as_bytes()
            .cmp(right.name.as_str().as_bytes())
    });
    values.projects = projects;
}

fn normalize_list(list: &mut Vec<NonBlankString>) {
    let mut kept = Vec::<NonBlankString>::new();
    for mut value in list.drain(..) {
        value.normalize_single_line();
        if !kept
            .iter()
            .any(|existing| same_value(existing.as_str(), value.as_str()))
        {
            kept.push(value);
        }
    }
    kept.sort_by(|left, right| left.as_str().as_bytes().cmp(right.as_str().as_bytes()));
    *list = kept;
}

/// Values of one kind with how many Orders use each, the first spelling met kept.
#[derive(Default)]
struct Tally {
    entries: Vec<ReusableValueProjection>,
    positions: BTreeMap<ValueKey, usize>,
}

impl Tally {
    /// Holds `value` whether or not an Order uses it.
    fn keep(&mut self, value: &str) {
        self.position(value);
    }

    /// Counts one Order using `value`.
    fn count(&mut self, value: &str) {
        if let Some(position) = self.position(value) {
            self.entries[position].usage_count += 1;
        }
    }

    /// This tally's spelling of `value`.
    fn spelling(&self, value: &str) -> Option<&str> {
        let position = *self.positions.get(&ValueKey::of(value)?)?;
        Some(&self.entries[position].value)
    }

    fn position(&mut self, value: &str) -> Option<usize> {
        let key = ValueKey::of(value)?;
        let next = self.entries.len();
        let position = *self.positions.entry(key.clone()).or_insert(next);
        if position == next {
            self.entries.push(ReusableValueProjection {
                value: key.0,
                usage_count: 0,
            });
        }
        Some(position)
    }

    fn finish(mut self) -> Vec<ReusableValueProjection> {
        self.entries
            .sort_by(|left, right| left.value.as_bytes().cmp(right.value.as_bytes()));
        self.entries
    }
}

/// A Package's values, kept or used, each with how many healthy Orders use it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ValueProjections {
    pub ordering_institutions: Vec<ReusableValueProjection>,
    pub orderers: Vec<ReusableValueProjection>,
    pub projects: Vec<ReusableValueProjection>,
    /// Every Order Group name across Projects, a name used under several counted once.
    pub order_groups: Vec<ReusableValueProjection>,
    /// Each Project's Order Groups under that Project's spelling, in byte order.
    project_groups: Vec<(String, Vec<ReusableValueProjection>)>,
    /// The Order Groups of Orders with no Project.
    projectless_groups: Vec<ReusableValueProjection>,
}

impl ValueProjections {
    /// The Order Groups of `project`, or of the Orders with no Project for `None`.
    pub(crate) fn order_groups_of(&self, project: Option<&str>) -> &[ReusableValueProjection] {
        match project.map(str::trim).filter(|project| !project.is_empty()) {
            None => &self.projectless_groups,
            Some(project) => self
                .project_groups
                .iter()
                .find(|(name, _)| same_value(name, project))
                .map_or(&[], |(_, groups)| groups),
        }
    }
}

/// The values `stored` keeps and `orders` use.
pub(crate) fn project<'a>(
    stored: &StoredValues,
    orders: impl Iterator<Item = &'a OrderDocument> + Clone,
) -> ValueProjections {
    let mut institutions = Tally::default();
    let mut orderers = Tally::default();
    let mut projects = Tally::default();
    let mut all_groups = Tally::default();
    let mut projectless = Tally::default();
    let mut groups = BTreeMap::<ValueKey, Tally>::new();

    for value in &stored.ordering_institutions {
        institutions.keep(value.as_str());
    }
    for value in &stored.orderers {
        orderers.keep(value.as_str());
    }
    for value in &stored.order_groups {
        projectless.keep(value.as_str());
        all_groups.keep(value.as_str());
    }
    for project in &stored.projects {
        projects.keep(project.name.as_str());
        let Some(key) = ValueKey::of(project.name.as_str()) else {
            continue;
        };
        let tally = groups.entry(key).or_default();
        for group in &project.order_groups {
            tally.keep(group.as_str());
            all_groups.keep(group.as_str());
        }
    }

    for order in orders {
        if let Some(value) = &order.ordering_institution {
            institutions.count(value);
        }
        if let Some(value) = &order.orderer {
            orderers.count(value);
        }
        let project = order.project.as_deref().and_then(ValueKey::of);
        if let Some(project) = &project {
            projects.count(&project.0);
        }
        if let Some(group) = order.order_group.as_deref() {
            all_groups.count(group);
            match project {
                Some(project) => groups.entry(project).or_default().count(group),
                None => projectless.count(group),
            }
        }
    }

    let mut project_groups = groups
        .into_iter()
        .map(|(key, tally)| {
            let name = projects.spelling(&key.0).unwrap_or(&key.0).to_owned();
            (name, tally.finish())
        })
        .collect::<Vec<_>>();
    project_groups.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    ValueProjections {
        ordering_institutions: institutions.finish(),
        orderers: orderers.finish(),
        projects: projects.finish(),
        order_groups: all_groups.finish(),
        project_groups,
        projectless_groups: projectless.finish(),
    }
}

/// The values a change to one Order adds to those the Package keeps: each of its values
/// that is new.
pub(crate) fn additions(values: &ReusableValues) -> StoredValues {
    let mut kept = StoredValues::default();
    let new = |value: &ReusableValue| match value {
        ReusableValue::New(value) => NonBlankString::trimmed(value),
        _ => None,
    };
    kept.ordering_institutions
        .extend(new(&values.ordering_institution));
    kept.orderers.extend(new(&values.orderer));
    let project = match &values.project {
        ReusableValue::Reused(name) | ReusableValue::New(name) => NonBlankString::trimmed(name),
        ReusableValue::Absent => None,
    };
    let group = new(&values.order_group);
    match project {
        Some(name) if group.is_some() || new(&values.project).is_some() => {
            kept.projects.push(StoredProject {
                name,
                order_groups: group.into_iter().collect(),
            });
        }
        Some(_) => {}
        None => kept.order_groups.extend(group),
    }
    kept
}

/// `additions` kept beside `values`.
pub(crate) fn merged(values: &StoredValues, additions: &StoredValues) -> StoredValues {
    let mut merged = values.clone();
    merged
        .ordering_institutions
        .extend(additions.ordering_institutions.iter().cloned());
    merged.orderers.extend(additions.orderers.iter().cloned());
    merged.projects.extend(additions.projects.iter().cloned());
    merged
        .order_groups
        .extend(additions.order_groups.iter().cloned());
    normalize(&mut merged);
    merged
}

/// The canonical values replacement that must commit with the Order introducing them.
pub(crate) struct ValuesWrite {
    root: PathBuf,
    path: PathBuf,
    prior: Vec<u8>,
    candidate: Vec<u8>,
}

impl ValuesWrite {
    pub(crate) fn prepare<F: FileSystem>(
        package: &CurrentPackageCandidate,
        additions: &StoredValues,
        filesystem: &F,
    ) -> Result<Option<Self>, Failure> {
        if additions.is_empty() {
            return Ok(None);
        }
        let values = merged(&package.values, additions);
        if values == package.values {
            return Ok(None);
        }
        let root = package.context.root.absolute_path.clone();
        let path = package.context.vendors.absolute_path.clone();
        let prior = filesystem.read(&path).map_err(|error| {
            Failure::from(FailedOperation::inside(
                &root,
                path.clone(),
                &error,
                filesystem,
            ))
        })?;
        let holds_loaded = VendorsDocument::from_json_bytes(&prior)
            .ok()
            .and_then(|document| PackageMetadata::new(&[], &document).validate().ok())
            .is_some_and(|projected| {
                projected.vendors().vendors == package.vendors
                    && projected.vendors().values == package.values
            });
        if !holds_loaded {
            return Err(Failure::PreconditionsChanged {
                location: located_in_package(&root, path),
            });
        }
        let candidate = VendorsDocument {
            schema_version: Version1,
            vendors: package.vendors.clone(),
            values,
        }
        .to_json_bytes()
        .map_err(|_| Failure::InvalidAggregate)?;
        Ok(Some(Self {
            root,
            path,
            prior,
            candidate,
        }))
    }

    pub(crate) fn replacement(&self) -> FileReplacement<'_> {
        FileReplacement::observed(&self.path, &self.candidate, &self.prior)
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub(crate) fn write<F: FileSystem>(&self, filesystem: &F) -> Result<(), Failure> {
        replace_validated_file_expecting(
            &self.path,
            &self.candidate,
            DestinationExpectation::Exactly(&self.prior),
            filesystem,
            |_| Ok::<(), Never>(()),
        )
        .map_err(|error| Failure::of_replacement(&self.root, &self.path, error))
    }

    pub(crate) fn rollback<F: FileSystem>(&self, failure: Failure, filesystem: &F) -> Failure {
        let mut cleanup = Vec::new();
        if let Err(error) = replace_validated_file_expecting(
            &self.path,
            &self.prior,
            DestinationExpectation::Exactly(&self.candidate),
            filesystem,
            |_| Ok::<(), Never>(()),
        ) {
            cleanup.push(CleanupFailure::new(located_in_package(
                &self.root,
                self.path.clone(),
            )));
            if error.cleanup_failure.is_some() {
                cleanup.push(CleanupFailure::new(located_in_package(
                    &self.root,
                    staging_path(&self.path).expect("a managed record has a staging path"),
                )));
            }
        }
        with_cleanup(
            failure,
            cleanup,
            located_in_package(&self.root, self.path.clone()),
        )
    }
}

/// Keeps cleanup failures beside the initiating commit failure.
pub(crate) fn with_cleanup(
    failure: Failure,
    mut cleanup: Vec<CleanupFailure>,
    at: Location,
) -> Failure {
    if cleanup.is_empty() {
        return failure;
    }
    match failure {
        Failure::CommitFailed { failure } => Failure::commit(*failure, cleanup),
        Failure::CommitFailedCleanupIncomplete {
            failure,
            cleanup_failures,
        } => {
            cleanup.extend(cleanup_failures);
            Failure::commit(*failure, cleanup)
        }
        _ => Failure::commit(CommitFailure::new(at), cleanup),
    }
}
