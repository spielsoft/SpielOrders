//! The values a Package keeps beside its Vendors: Ordering Institutions, Orderers,
//! Projects, and each Project's Order Groups — and the one vocabulary every capability
//! names them in: their four kinds, where each kind lives ([`ValueScope`]), and how a proposed
//! spelling reuses a kept one or is new ([`ReusableValue`]).
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
    journal::ManagedRecord,
    metadata::{
        NonBlankString, OrderDocument, StoredProject, StoredValues, VendorsDocument, Version1,
    },
    operations::FailedOperation,
    ports::FileSystem,
    readme::located_in_package,
};
use std::{cmp::Ordering, collections::BTreeMap, path::PathBuf};

/// One of the four Reusable Free-Entry Value categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReusableValueKind {
    OrderingInstitution,
    Orderer,
    Project,
    OrderGroup,
}

impl ReusableValueKind {
    /// Every kind, in the order an Order's facts list them.
    pub(crate) const ALL: [Self; 4] = [
        Self::Orderer,
        Self::OrderingInstitution,
        Self::Project,
        Self::OrderGroup,
    ];

    /// The Order field that holds a value of this kind.
    pub(crate) fn field(self) -> &'static str {
        match self {
            Self::OrderingInstitution => "ordering_institution",
            Self::Orderer => "orderer",
            Self::Project => "project",
            Self::OrderGroup => "order_group",
        }
    }
}

/// How one proposed value is spelled against the values a Package already has.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReusableValue {
    Absent,
    Reused(String),
    New(String),
}

impl ReusableValue {
    /// The spelling this value stores, where it stores one.
    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Self::Absent => None,
            Self::Reused(value) | Self::New(value) => Some(value),
        }
    }
}

/// How a mutation spells each of an Order's Reusable Free-Entry Values: reusing an
/// existing canonical spelling, introducing a new one, or leaving it absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReusableValues {
    pub orderer: ReusableValue,
    pub ordering_institution: ReusableValue,
    pub project: ReusableValue,
    pub order_group: ReusableValue,
}

impl ReusableValues {
    /// The value of `kind`.
    pub(crate) fn of(&self, kind: ReusableValueKind) -> &ReusableValue {
        match kind {
            ReusableValueKind::OrderingInstitution => &self.ordering_institution,
            ReusableValueKind::Orderer => &self.orderer,
            ReusableValueKind::Project => &self.project,
            ReusableValueKind::OrderGroup => &self.order_group,
        }
    }

    /// Each `proposed` value as `package` spells it, an Order Group among the groups of
    /// the Project these values name.
    pub(crate) fn canonical<'a>(
        package: &CurrentPackageCandidate,
        proposed: impl Fn(ReusableValueKind) -> Option<&'a str>,
    ) -> Self {
        let canonical =
            |kind, project| ValueScope::new(kind, project).canonical(package, proposed(kind));
        let project = canonical(ReusableValueKind::Project, None);
        Self {
            orderer: canonical(ReusableValueKind::Orderer, None),
            ordering_institution: canonical(ReusableValueKind::OrderingInstitution, None),
            order_group: canonical(ReusableValueKind::OrderGroup, project.text()),
            project,
        }
    }

    /// The values `document` holds as `package` spells them.
    pub(crate) fn of_document(package: &CurrentPackageCandidate, document: &OrderDocument) -> Self {
        Self::canonical(package, |kind| held(document, kind))
    }

    /// Every value with its kind, in [`ReusableValueKind::ALL`]'s order.
    pub(crate) fn each(&self) -> impl Iterator<Item = (ReusableValueKind, &ReusableValue)> {
        ReusableValueKind::ALL
            .into_iter()
            .map(|kind| (kind, self.of(kind)))
    }
}

/// `proposed` reusing the first of `existing_values` it matches, new where it matches
/// none, or absent where it is blank.
pub fn canonicalize_reusable_value<T: AsRef<str>>(
    proposed: Option<&str>,
    existing_values: &[T],
) -> ReusableValue {
    let Some(proposed) = proposed.map(str::trim).filter(|value| !value.is_empty()) else {
        return ReusableValue::Absent;
    };

    existing_values
        .iter()
        .map(AsRef::as_ref)
        .find(|existing| same_value(proposed, existing))
        .map(|existing| ReusableValue::Reused(existing.trim().to_owned()))
        .unwrap_or_else(|| ReusableValue::New(proposed.to_owned()))
}

/// The value of `kind` `document` holds, whatever its Project.
pub(crate) fn held(document: &OrderDocument, kind: ReusableValueKind) -> Option<&str> {
    match kind {
        ReusableValueKind::OrderingInstitution => document.ordering_institution.as_deref(),
        ReusableValueKind::Orderer => document.orderer.as_deref(),
        ReusableValueKind::Project => document.project.as_deref(),
        ReusableValueKind::OrderGroup => document.order_group.as_deref(),
    }
}

/// Whether two spellings are one value: trimmed, and without regard to letter case.
pub(crate) fn same_value(left: &str, right: &str) -> bool {
    focaccia::unicode_full_case_eq(left.trim(), right.trim())
}

/// Whether two optional values are one, a blank value being none: so two Orders with no
/// Project are in the same Project.
pub(crate) fn same_optional(left: Option<&str>, right: Option<&str>) -> bool {
    fn present(text: Option<&str>) -> Option<&str> {
        text.map(str::trim).filter(|text| !text.is_empty())
    }
    match (present(left), present(right)) {
        (None, None) => true,
        (Some(left), Some(right)) => same_value(left, right),
        _ => false,
    }
}

/// Where one kind of value lives: a list of its own, or the Order Groups of one Project or
/// of the Orders with no Project.
#[derive(Clone, Debug)]
pub(crate) enum ValueScope {
    OrderingInstitution,
    Orderer,
    Project,
    OrderGroup { project: Option<String> },
}

impl ValueScope {
    /// The scope of `kind` in `package`, an Order Group's naming a Project the Package
    /// has; a blank Project is the Orders with no Project.
    pub(crate) fn of(
        kind: ReusableValueKind,
        project: Option<&str>,
        package: &CurrentPackageCandidate,
    ) -> Result<Self, Failure> {
        Ok(match Self::new(kind, None) {
            Self::OrderGroup { .. } => Self::OrderGroup {
                project: match project.map(str::trim).filter(|project| !project.is_empty()) {
                    None => None,
                    Some(project) => Some(Self::Project.known(package, project)?),
                },
            },
            scope => scope,
        })
    }

    /// The scope of `kind`, an Order Group's being the groups of `project` as given.
    pub(crate) fn new(kind: ReusableValueKind, project: Option<&str>) -> Self {
        match kind {
            ReusableValueKind::OrderingInstitution => Self::OrderingInstitution,
            ReusableValueKind::Orderer => Self::Orderer,
            ReusableValueKind::Project => Self::Project,
            ReusableValueKind::OrderGroup => Self::OrderGroup {
                project: project
                    .map(str::trim)
                    .filter(|project| !project.is_empty())
                    .map(str::to_owned),
            },
        }
    }

    pub(crate) fn kind(&self) -> ReusableValueKind {
        match self {
            Self::OrderingInstitution => ReusableValueKind::OrderingInstitution,
            Self::Orderer => ReusableValueKind::Orderer,
            Self::Project => ReusableValueKind::Project,
            Self::OrderGroup { .. } => ReusableValueKind::OrderGroup,
        }
    }

    /// The Order field this scope's values are held in.
    pub(crate) fn field(&self) -> &'static str {
        self.kind().field()
    }

    /// The values `projections` lists in this scope.
    pub(crate) fn listed<'a>(
        &self,
        projections: &'a ValueProjections,
    ) -> &'a [ReusableValueProjection] {
        match self {
            Self::OrderingInstitution => &projections.ordering_institutions,
            Self::Orderer => &projections.orderers,
            Self::Project => &projections.projects,
            Self::OrderGroup { project } => projections.order_groups_of(project.as_deref()),
        }
    }

    /// `proposed` as this scope of `package` spells it: reused, new, or absent.
    pub(crate) fn canonical(
        &self,
        package: &CurrentPackageCandidate,
        proposed: Option<&str>,
    ) -> ReusableValue {
        let listed = self
            .listed(&package.projections)
            .iter()
            .map(|listed| listed.value.as_str())
            .collect::<Vec<_>>();
        canonicalize_reusable_value(proposed, &listed)
    }

    /// The Package's spelling of `value` in this scope, where it has one.
    pub(crate) fn existing(
        &self,
        package: &CurrentPackageCandidate,
        value: &str,
    ) -> Option<String> {
        self.listed(&package.projections)
            .iter()
            .find(|existing| same_value(&existing.value, value))
            .map(|existing| existing.value.clone())
    }

    /// The Package's spelling of `value`, or the refusal naming it as unknown.
    pub(crate) fn known(
        &self,
        package: &CurrentPackageCandidate,
        value: &str,
    ) -> Result<String, Failure> {
        self.existing(package, value)
            .ok_or_else(|| Failure::UnknownValue {
                field: self.field().to_owned(),
                value: value.trim().to_owned(),
            })
    }

    /// The text of this scope's value on `order`, where the Order is in this scope.
    pub(crate) fn value_of<'a>(&self, order: &'a OrderDocument) -> Option<&'a str> {
        match self {
            Self::OrderGroup { project } => {
                same_optional(order.project.as_deref(), project.as_deref())
                    .then_some(order.order_group.as_deref())
                    .flatten()
            }
            scope => held(order, scope.kind()),
        }
    }

    /// Sets this scope's value on `order`.
    pub(crate) fn set(&self, order: &mut OrderDocument, value: Option<&str>) {
        let value = value.map(str::to_owned);
        match self {
            Self::OrderingInstitution => order.ordering_institution = value,
            Self::Orderer => order.orderer = value,
            Self::Project => order.project = value,
            Self::OrderGroup { .. } => order.order_group = value,
        }
    }

    /// The positions of the healthy Orders using one of `values`.
    pub(crate) fn users(&self, package: &CurrentPackageCandidate, values: &[String]) -> Vec<usize> {
        package
            .healthy_orders
            .iter()
            .enumerate()
            .filter(|(_, order)| {
                self.value_of(&order.document)
                    .is_some_and(|own| values.iter().any(|value| same_value(own, value)))
            })
            .map(|(position, _)| position)
            .collect()
    }

    /// Keeps `value` in this scope.
    pub(crate) fn keep(&self, values: &mut StoredValues, value: &str) {
        let Some(value) = NonBlankString::trimmed(value) else {
            return;
        };
        match self {
            Self::OrderingInstitution => values.ordering_institutions.push(value),
            Self::Orderer => values.orderers.push(value),
            Self::Project => values.projects.push(StoredProject {
                name: value,
                order_groups: Vec::new(),
            }),
            Self::OrderGroup { project: None } => values.order_groups.push(value),
            Self::OrderGroup {
                project: Some(project),
            } => match values
                .projects
                .iter_mut()
                .find(|kept| same_value(kept.name.as_str(), project))
            {
                Some(kept) => kept.order_groups.push(value),
                None => values.projects.push(StoredProject {
                    name: NonBlankString::trimmed(project).expect("a known Project is nonblank"),
                    order_groups: vec![value],
                }),
            },
        }
    }
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

/// Whether `order` uses `value` of `kind`, as [`project`] counts it: an Order Group only
/// within `project`, or among the Orders with no Project where `project` is absent or blank.
pub(crate) fn uses(
    order: &OrderDocument,
    kind: ReusableValueKind,
    project: Option<&str>,
    value: &str,
) -> bool {
    let key = |text: Option<&str>| text.and_then(ValueKey::of);
    let matches = |own: Option<&str>| key(own).is_some_and(|own| Some(own) == key(Some(value)));
    match kind {
        ReusableValueKind::OrderingInstitution => matches(order.ordering_institution.as_deref()),
        ReusableValueKind::Orderer => matches(order.orderer.as_deref()),
        ReusableValueKind::Project => matches(order.project.as_deref()),
        ReusableValueKind::OrderGroup => {
            key(order.project.as_deref()) == key(project) && matches(order.order_group.as_deref())
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

/// The replacement of the Vendor record that keeps the values an Order introduces, and
/// must commit with that Order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Replacement {
    pub(crate) path: PathBuf,
    /// What the record holds now, which the replacement expects to still find.
    pub(crate) prior: Vec<u8>,
    pub(crate) candidate: Vec<u8>,
}

impl Replacement {
    /// The replacement a plan lists, before a commit reads the record it replaces.
    pub(crate) fn planned(package: &CurrentPackageCandidate) -> Self {
        Self {
            path: package.context.vendors.absolute_path.clone(),
            prior: Vec::new(),
            candidate: Vec::new(),
        }
    }
}

/// The Vendor record of `package` keeping `additions` beside the values it keeps, or
/// `None` where it already keeps them all. Refused where the record no longer holds what
/// `package` loaded from it.
pub(crate) fn replacement<F: FileSystem>(
    package: &CurrentPackageCandidate,
    additions: &StoredValues,
    filesystem: &F,
) -> Result<Option<Replacement>, Failure> {
    if additions.is_empty() {
        return Ok(None);
    }
    let values = merged(&package.values, additions);
    if values == package.values {
        return Ok(None);
    }
    let root = &package.context.root.absolute_path;
    let path = package.context.vendors.absolute_path.clone();
    let prior = filesystem.read(&path).map_err(|error| {
        Failure::from(FailedOperation::inside(
            root,
            path.clone(),
            &error,
            filesystem,
        ))
    })?;
    let loaded = VendorsDocument {
        schema_version: Version1,
        vendors: package.vendors.clone(),
        values: package.values.clone(),
    };
    if !ManagedRecord::vendors(path.clone(), loaded).held_by(&prior) {
        return Err(Failure::PreconditionsChanged {
            location: located_in_package(root, path),
        });
    }
    let candidate = VendorsDocument {
        schema_version: Version1,
        vendors: package.vendors.clone(),
        values,
    }
    .to_json_bytes()
    .map_err(|_| Failure::InvalidAggregate)?;
    Ok(Some(Replacement {
        path,
        prior,
        candidate,
    }))
}
