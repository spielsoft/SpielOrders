//! An Order's facts as a caller assigns them, and the one resolution that turns an
//! accepted assignment set into the record creation, adoption, and an edit write.
//!
//! **One vocabulary and one resolution for every capability that writes an Order's
//! facts.** Which fields an Order and its Items have, which of them may be assigned, which
//! rule each refusal names, how a Vendor Name selects a Vendor, and how each value reuses a
//! kept spelling are Package rules. Creation, adoption, and an edit differ only in what the
//! facts start from — nothing, or the stored record — so each states that and resolves its
//! set here. Two copies is how a field comes to be assignable on one command and unknown on
//! another, or a value reused by one capability and new to another.
//!
//! Decision sources: Define the Order record and lifecycle contract; Define the Rust CLI
//! capability contract.

use crate::{
    Assignments, Failure,
    answer::DraftFailure,
    canonical::CurrentPackageCandidate,
    change::Assignment,
    code::Coded,
    evidence::EvidenceFingerprint,
    metadata::{
        Currency, DateOrdered, OrderDocument, OrderId, OrderItem, OrderStatus, StoredValues, Vendor,
    },
    mutation::{PLANNING_IDENTITY, RejectedAssignment, RejectedAssignmentReason},
    ports::LocalDate,
    values::{ReusableValue, ReusableValueKind, ReusableValues, ValueScope, same_optional},
    vendors::{vendor_by_name, vendors_location},
};
use serde::Serialize;

/// Every fact of an Order a caller may assign, in the order an editor lists them.
///
/// `Vendor` is the Vendor *Name*, which is the ordinary Vendor selector. The record stores
/// the Vendor ID it resolves to; nothing here takes an identity, because a surface that
/// holds identities back offers no way to learn one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrderField {
    Title,
    DateOrdered,
    Status,
    Vendor,
    Orderer,
    OrderingInstitution,
    Currency,
    Project,
    OrderGroup,
}

impl OrderField {
    pub(crate) const ALL: [Self; 9] = [
        Self::Title,
        Self::DateOrdered,
        Self::Status,
        Self::Vendor,
        Self::Orderer,
        Self::OrderingInstitution,
        Self::Currency,
        Self::Project,
        Self::OrderGroup,
    ];

    /// The name a caller assigns this field by.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::DateOrdered => "date_ordered",
            Self::Status => "status",
            Self::Vendor => "vendor",
            Self::Orderer => "orderer",
            Self::OrderingInstitution => "ordering_institution",
            Self::Currency => "currency",
            Self::Project => "project",
            Self::OrderGroup => "order_group",
        }
    }

    /// The field `name` assigns.
    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.name() == name)
    }

    /// The kind of Reusable Free-Entry Value this field holds, where it holds one.
    pub(crate) fn reusable(self) -> Option<ReusableValueKind> {
        match self {
            Self::Orderer => Some(ReusableValueKind::Orderer),
            Self::OrderingInstitution => Some(ReusableValueKind::OrderingInstitution),
            Self::Project => Some(ReusableValueKind::Project),
            Self::OrderGroup => Some(ReusableValueKind::OrderGroup),
            _ => None,
        }
    }

    /// Whether every Order holds this fact, so a blank value is refused.
    fn required(self) -> bool {
        matches!(self, Self::Title | Self::DateOrdered | Self::Status)
    }
}

impl From<ReusableValueKind> for OrderField {
    fn from(kind: ReusableValueKind) -> Self {
        match kind {
            ReusableValueKind::Orderer => Self::Orderer,
            ReusableValueKind::OrderingInstitution => Self::OrderingInstitution,
            ReusableValueKind::Project => Self::Project,
            ReusableValueKind::OrderGroup => Self::OrderGroup,
        }
    }
}

/// Every fact of an Order Item a caller may assign, in the order an editor lists them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ItemField {
    Description,
    Quantity,
    UnitPrice,
    CatalogNumber,
    Notes,
}

impl ItemField {
    pub(crate) const ALL: [Self; 5] = [
        Self::Description,
        Self::Quantity,
        Self::UnitPrice,
        Self::CatalogNumber,
        Self::Notes,
    ];

    /// The name a caller assigns this field by.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Description => "description",
            Self::Quantity => "quantity",
            Self::UnitPrice => "unit_price",
            Self::CatalogNumber => "catalog_number",
            Self::Notes => "notes",
        }
    }

    /// The field `name` assigns.
    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.name() == name)
    }

    /// Whether every Item holds this fact.
    fn required(self) -> bool {
        matches!(self, Self::Description | Self::Quantity | Self::UnitPrice)
    }
}

/// One Order fact whose value the record constrains rather than merely stores.
///
/// **A refusal has to name which fact was wrong, and the assembled record cannot.** A
/// candidate Order either parses and validates or it does not, so a refusal read off it
/// says only that some fact of the set was unacceptable — and an atomic set is exactly
/// where that is least useful. These three are the facts a caller can get wrong that way:
/// an Order Status outside the established five, a Date Ordered that is not a real
/// calendar day, and an Order Currency that is not a three-letter code.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderFactKind {
    Status,
    DateOrdered,
    Currency,
}

impl OrderFactKind {
    const ALL: [Self; 3] = [Self::Status, Self::DateOrdered, Self::Currency];

    /// Whether the Order record can hold this text as this fact.
    ///
    /// Asked of the record's own value types, and for a Date Ordered of the same calendar
    /// rule [`OrderDocument::validate`] applies — so a value refused here and a stored
    /// value a Package load rejects cannot come to disagree.
    fn holds(self, value: &str) -> bool {
        let held = |value: &str| serde_json::Value::String(value.to_owned());
        match self {
            Self::Status => parse_status(value).is_some(),
            Self::DateOrdered => serde_json::from_value::<DateOrdered>(held(value))
                .is_ok_and(|date| date.is_real_gregorian()),
            Self::Currency => serde_json::from_value::<Currency>(held(value)).is_ok(),
        }
    }

    fn field(self) -> OrderField {
        match self {
            Self::Status => OrderField::Status,
            Self::DateOrdered => OrderField::DateOrdered,
            Self::Currency => OrderField::Currency,
        }
    }
}

impl Coded for OrderFactKind {}

/// One assignment a set's adjudication accepted.
#[derive(Clone, Debug)]
struct Accepted {
    field: OrderField,
    value: String,
    /// Whether the value is to be added where the Package lacks it.
    introduce: bool,
}

/// One canonical assignment set adjudicated against the Order's own fields: the
/// assignments it accepted, and every rule it broke whatever Package it is resolved in.
#[derive(Clone, Debug)]
pub(crate) struct OrderAssignments {
    accepted: Vec<Accepted>,
    faults: Vec<Failure>,
}

impl OrderAssignments {
    /// Adjudicates one set: every refused assignment, then each accepted fact the record
    /// cannot hold, in that order.
    pub(crate) fn of(assignments: &Assignments) -> Self {
        let mut rejected = Vec::new();
        let mut accepted = Vec::<Accepted>::new();
        for Assignment {
            field: name,
            value,
            confirmed,
        } in &assignments.entries
        {
            let reason = match order_assignment_refusal(name, value) {
                Some(RejectedAssignmentReason::BlankRequiredValue) | None
                    if accepted.iter().any(|each| each.field.name() == name) =>
                {
                    Some(RejectedAssignmentReason::Duplicate)
                }
                Some(RejectedAssignmentReason::BlankRequiredValue) | None
                    if *confirmed
                        && OrderField::named(name)
                            .and_then(OrderField::reusable)
                            .is_none() =>
                {
                    Some(RejectedAssignmentReason::NotAReusableValue)
                }
                reason => reason,
            };
            match (reason, OrderField::named(name)) {
                (None, Some(field)) => accepted.push(Accepted {
                    field,
                    value: value.clone(),
                    introduce: *confirmed,
                }),
                (reason, _) => rejected.push(RejectedAssignment::new(
                    name.clone(),
                    reason.unwrap_or(RejectedAssignmentReason::UnknownField),
                )),
            }
        }
        let mut faults = Vec::new();
        if !rejected.is_empty() {
            faults.push(Failure::InvalidAssignments(rejected));
        }
        let set = Self { accepted, faults };
        // The three constrained facts are named here, whichever capability is given one
        // the record cannot hold: an assembled candidate that failed to parse could only
        // say that some fact was wrong.
        let unholdable = OrderFactKind::ALL
            .into_iter()
            .filter_map(|fact| {
                let value = set
                    .value(fact.field())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?;
                (!fact.holds(value)).then(|| Failure::InvalidFactValue {
                    fact,
                    value: value.to_owned(),
                })
            })
            .collect::<Vec<_>>();
        Self {
            faults: set.faults.into_iter().chain(unholdable).collect(),
            ..set
        }
    }

    /// Every rule this set broke on its own, in adjudication order.
    pub(crate) fn faults(&self) -> &[Failure] {
        &self.faults
    }

    /// The set `assignments` names for an Order with no record yet, with every rule it broke
    /// on its own among its faults: each refused assignment and unholdable fact, and every
    /// one of `required` it does not assign at all. Planning reports them beside the Vendor, value,
    /// and row faults it meets, as an edit does.
    pub(crate) fn new_order(assignments: &Assignments, required: &[OrderField]) -> Self {
        let mut set = Self::of(assignments);
        // A field assigned and refused is reported as refused, not missing as well.
        let missing = required
            .iter()
            .filter(|field| {
                !assignments
                    .entries
                    .iter()
                    .any(|entry| entry.field == field.name())
            })
            .map(|field| field.name().to_owned())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            set.faults
                .push(Failure::MissingRequiredFacts { fields: missing });
        }
        set
    }

    /// The value this set assigns `field`, as given.
    fn value(&self, field: OrderField) -> Option<&str> {
        self.accepted
            .iter()
            .find(|each| each.field == field)
            .map(|each| each.value.as_str())
    }

    /// Whether this set asks that `field`'s value be added where the Package lacks it.
    fn introduces(&self, field: OrderField) -> bool {
        self.accepted
            .iter()
            .any(|each| each.field == field && each.introduce)
    }

    /// The Order Status this set assigns, where it assigns one the record can hold.
    fn status(&self) -> Option<OrderStatus> {
        self.value(OrderField::Status).and_then(parse_status)
    }
}

/// What an Order's facts start from before a set is resolved over them.
pub(crate) enum Prior<'a> {
    /// An Order with no record yet. A Date Ordered it is not given is today's, and a
    /// Status it is not given is [`OrderCreationDefaults::STATUS`].
    New { today: &'a dyn Fn() -> LocalDate },
    /// The stored record of an existing Order. A Date Ordered it is not given follows a
    /// change of Status as an editor's does.
    Stored {
        document: &'a OrderDocument,
        today: &'a dyn Fn() -> LocalDate,
    },
}

/// The Date Ordered and Status a new Order takes when its set assigns neither.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OrderCreationDefaults {
    pub date_ordered: String,
    pub status: OrderStatus,
}

impl OrderCreationDefaults {
    pub(crate) const STATUS: OrderStatus = OrderStatus::InPreparation;

    pub fn new(today: &dyn Fn() -> LocalDate) -> Self {
        Self {
            date_ordered: date_text(today()),
            status: Self::STATUS,
        }
    }
}

/// One Order's facts resolved in its Package: the record a change writes, the Vendor it
/// references, how it spells each reusable value, and the values it adds to those the
/// Package keeps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OrderFacts {
    pub(crate) document: OrderDocument,
    pub(crate) vendor: Option<Vendor>,
    pub(crate) values: ReusableValues,
    /// The new values this change adds to those the Package keeps.
    pub(crate) additions: StoredValues,
    /// What the resolution relied on in its Package, re-observed before a commit.
    evidence: EvidenceFingerprint,
}

impl OrderFacts {
    /// The facts `set` and `rows` give an Order starting from `prior` in `package`, or
    /// every rule they broke: the set's own, then its Vendor's and each value's, then each
    /// row's. A record that cannot hold what was resolved is refused only when nothing
    /// else was.
    pub(crate) fn resolve(
        package: &CurrentPackageCandidate,
        prior: Prior<'_>,
        set: &OrderAssignments,
        rows: &[Assignments],
    ) -> Result<Self, Vec<DraftFailure>> {
        let at_facts = |failure| DraftFailure { row: None, failure };
        let mut faults = set.faults().to_vec();
        let (mut object, prior_project) = match &prior {
            Prior::New { today } => {
                let status = set.status().unwrap_or(OrderCreationDefaults::STATUS);
                let date_ordered = set
                    .value(OrderField::DateOrdered)
                    .map(|date| date.trim().to_owned())
                    .unwrap_or_else(|| OrderCreationDefaults::new(*today).date_ordered);
                let mut object = serde_json::Map::new();
                object.insert("schema_version".into(), 1.into());
                object.insert("package_root".into(), package_root_locator(package).into());
                object.insert("order_id".into(), PLANNING_IDENTITY.into());
                object.insert("date_ordered".into(), date_ordered.into());
                object.insert(
                    "status".into(),
                    serde_json::to_value(status).expect("an Order Status serializes"),
                );
                (object, None)
            }
            Prior::Stored { document, today } => {
                // An assigned `status` the record cannot hold was refused by the set's own
                // adjudication, so one that does not parse here keeps the Order's own, as a
                // set that assigned none does.
                let status = set.status().unwrap_or(document.status);
                let date_ordered = match set.value(OrderField::DateOrdered) {
                    Some(date) => date.trim().to_owned(),
                    None => date_ordered_after_status_change(
                        document.status,
                        status,
                        document.date_ordered.as_str(),
                        *today,
                    ),
                };
                let Some(mut object) = serde_json::to_value(document)
                    .ok()
                    .and_then(|value| value.as_object().cloned())
                else {
                    faults.push(Failure::InvalidFacts);
                    return Err(faults.into_iter().map(at_facts).collect());
                };
                object.insert("date_ordered".into(), date_ordered.into());
                (object, document.project.as_deref())
            }
        };
        for each in &set.accepted {
            let value = each.value.trim();
            match each.field {
                OrderField::DateOrdered => {}
                // The one assigned field whose name is not the record's. `vendor` selects
                // by Vendor Name and the record stores the Vendor ID that Name resolves to,
                // so the assignment is translated rather than copied — including the empty
                // value, which clears the stored Reference.
                OrderField::Vendor if value.is_empty() => {
                    object.remove("vendor_id");
                }
                OrderField::Vendor => match vendor_by_name(value, &package.vendors) {
                    Some(vendor) => {
                        object.insert("vendor_id".into(), vendor.vendor_id.as_str().into());
                    }
                    None => faults.push(Failure::UnknownVendor {
                        name: value.to_owned(),
                        scope: vendors_location(package),
                    }),
                },
                field if value.is_empty() => {
                    object.remove(field.name());
                }
                field => {
                    object.insert(field.name().into(), value.into());
                }
            }
        }
        let values = spell_values(package, set, &mut object, prior_project)
            .map_err(|unconfirmed| faults.extend(unconfirmed))
            .ok();
        let mut faults = faults.into_iter().map(at_facts).collect::<Vec<_>>();
        let items = DraftFailure::rows(rows, &mut faults, draft_item);
        object.insert(
            "items".into(),
            serde_json::to_value(&items).expect("Order Items serialize"),
        );
        let Some(values) = values.filter(|_| faults.is_empty()) else {
            return Err(faults);
        };
        let document = serde_json::from_value::<OrderDocument>(serde_json::Value::Object(object))
            .ok()
            .and_then(|document| document.validate().ok())
            .map(|validated| validated.order().clone())
            .ok_or_else(|| vec![at_facts(Failure::InvalidFacts)])?;
        Ok(Self {
            vendor: vendor_of(&document, package).cloned(),
            additions: crate::values::additions(&values),
            evidence: Self::observe(&document, package),
            values,
            document,
        })
    }

    /// What resolving `document`'s facts relies on in `package`: the configured Orders
    /// Directory, the name of the Vendor it references, and how each of its values is
    /// spelled.
    ///
    /// The canonical spelling *and* the reuse decision are both recorded, so a value that
    /// becomes reusable between planning and the commit changes its fact even though its own
    /// text did not; and the Vendor's *name* is, because a plan reports the Vendor it
    /// references and an Order Directory is named after it.
    pub(crate) fn observe(
        document: &OrderDocument,
        package: &CurrentPackageCandidate,
    ) -> EvidenceFingerprint {
        let fingerprint = EvidenceFingerprint::new();
        let fingerprint = match package.context.orders_directory.as_ref() {
            Some(orders) => fingerprint.observed(
                "orders_directory",
                orders.absolute_path.as_os_str().as_encoded_bytes(),
            ),
            None => fingerprint.absent("orders_directory"),
        };
        let mut fingerprint = match vendor_of(document, package) {
            Some(vendor) => fingerprint.observed("vendor_name", vendor.name.as_str()),
            None => fingerprint.absent("vendor_name"),
        };
        for (kind, value) in ReusableValues::of_document(package, document).each() {
            fingerprint = match value {
                ReusableValue::Absent => fingerprint.absent(kind.field()),
                ReusableValue::Reused(value) => {
                    fingerprint.observed(kind.field(), format!("reused:{value}"))
                }
                ReusableValue::New(value) => {
                    fingerprint.observed(kind.field(), format!("new:{value}"))
                }
            };
        }
        fingerprint
    }

    /// Refuses unless `current` still holds everything these facts were resolved against,
    /// naming each fact that moved.
    pub(crate) fn confirm(&self, current: &CurrentPackageCandidate) -> Result<(), Failure> {
        let outcome = self
            .evidence
            .confirm(&Self::observe(&self.document, current));
        if outcome.is_unchanged() {
            Ok(())
        } else {
            Err(Failure::PlanEvidenceUnconfirmed { outcome })
        }
    }

    /// This record under the Order ID `order_id`, where that is one an Order can hold.
    pub(crate) fn with_identity(&self, order_id: &str) -> Option<OrderDocument> {
        let order_id =
            serde_json::from_value::<OrderId>(serde_json::Value::String(order_id.to_owned()))
                .ok()?;
        let document = OrderDocument {
            order_id,
            ..self.document.clone()
        };
        document
            .validate()
            .ok()
            .map(|validated| validated.order().clone())
    }
}

/// The Vendor `document` references in `package`, or `None`.
///
/// `None` for an Order that references no Vendor, and for one whose stored Reference
/// resolves to nothing — an Unresolved Vendor Reference, which a Package load reports as
/// its own diagnostic. A set that *assigns* a Vendor this Package does not hold is refused
/// before this, so the two cases cannot be confused for one another.
fn vendor_of<'a>(
    document: &OrderDocument,
    package: &'a CurrentPackageCandidate,
) -> Option<&'a Vendor> {
    let vendor_id = document.vendor_id.as_ref()?;
    package
        .vendors
        .iter()
        .find(|vendor| vendor.vendor_id == *vendor_id)
}

/// The locator from an Order Directory of `package` back to its Root.
fn package_root_locator(package: &CurrentPackageCandidate) -> String {
    let depth = package
        .context
        .orders_directory
        .as_ref()
        .and_then(|orders| orders.package_relative_path.as_ref())
        .expect("a current Package has an Orders Directory inside it")
        .split('/')
        .count();
    std::iter::repeat_n("..", depth + 1)
        .collect::<Vec<_>>()
        .join("/")
}

/// Spells every Reusable Free-Entry Value the resulting Order carries and refuses each
/// genuinely new value the set did not ask to introduce.
///
/// An Order Group is judged among the groups of the resulting Project. An Order moved to
/// another Project loses a group the set did not name unless that Project already has it;
/// a named group is reused or introduced there like any other value.
fn spell_values(
    package: &CurrentPackageCandidate,
    set: &OrderAssignments,
    object: &mut serde_json::Map<String, serde_json::Value>,
    prior_project: Option<&str>,
) -> Result<ReusableValues, Vec<Failure>> {
    let held = |object: &serde_json::Map<String, serde_json::Value>, kind: ReusableValueKind| {
        object
            .get(kind.field())
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let mut unconfirmed = Vec::new();
    let mut spell = |object: &mut serde_json::Map<String, serde_json::Value>,
                     kind: ReusableValueKind| {
        let project = held(object, ReusableValueKind::Project);
        let proposed = held(object, kind);
        let mut outcome =
            ValueScope::new(kind, project.as_deref()).canonical(package, proposed.as_deref());
        let field = OrderField::from(kind);
        if kind == ReusableValueKind::OrderGroup
            && !same_optional(prior_project, project.as_deref())
            && set.value(field).is_none()
            && matches!(outcome, ReusableValue::New(_))
        {
            outcome = ReusableValue::Absent;
        }
        if let ReusableValue::New(proposed) = &outcome
            && !set.introduces(field)
        {
            unconfirmed.push(Failure::UnconfirmedReusableValue {
                field: kind.field().to_owned(),
                proposed: proposed.clone(),
            });
        }
        match outcome.text() {
            Some(value) => {
                object.insert(kind.field().into(), value.into());
            }
            None => {
                object.remove(kind.field());
            }
        }
        outcome
    };
    let orderer = spell(object, ReusableValueKind::Orderer);
    let ordering_institution = spell(object, ReusableValueKind::OrderingInstitution);
    let project = spell(object, ReusableValueKind::Project);
    let order_group = spell(object, ReusableValueKind::OrderGroup);
    if !unconfirmed.is_empty() {
        return Err(unconfirmed);
    }
    Ok(ReusableValues {
        orderer,
        ordering_institution,
        project,
        order_group,
    })
}

/// A calendar date as an Order records it.
fn date_text(date: LocalDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

/// The Date Ordered an Order holds once its Status changes from `previous` to `next`:
/// entering Ordered takes today's date, which the user may then change.
pub(crate) fn date_ordered_after_status_change(
    previous: OrderStatus,
    next: OrderStatus,
    current: &str,
    today: &dyn Fn() -> LocalDate,
) -> String {
    if next == OrderStatus::Ordered && previous != OrderStatus::Ordered {
        date_text(today())
    } else {
        current.to_owned()
    }
}

pub(crate) fn parse_status(value: &str) -> Option<OrderStatus> {
    serde_json::from_value(serde_json::Value::String(value.trim().to_owned())).ok()
}

/// The finite number an Item's Quantity or Unit Price text holds.
pub(crate) fn item_number(value: &str) -> Option<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

/// Why an Order refuses `field=value` whatever else its set assigns.
pub(crate) fn order_assignment_refusal(
    field: &str,
    value: &str,
) -> Option<RejectedAssignmentReason> {
    match field {
        "order_id" => Some(RejectedAssignmentReason::ImmutableIdentity),
        "schema_version" | "package_root" => Some(RejectedAssignmentReason::ManagedTechnicalField),
        "items" => Some(RejectedAssignmentReason::OrderedCollection),
        field => match OrderField::named(field) {
            None => Some(RejectedAssignmentReason::UnknownField),
            Some(field) if field.required() && value.trim().is_empty() => {
                Some(RejectedAssignmentReason::BlankRequiredValue)
            }
            Some(_) => None,
        },
    }
}

/// Why an Order Item refuses `field=value` whatever else its set assigns.
pub(crate) fn item_assignment_refusal(
    field: &str,
    value: &str,
) -> Option<RejectedAssignmentReason> {
    item_assignment(field, value, false, false).err()
}

/// One row of a whole draft as its Item, or every fault the row has: a required fact
/// the row does not assign is reported beside the values it rejected, not hidden by them.
pub(crate) fn draft_item(assignments: &Assignments) -> Result<OrderItem, Vec<Failure>> {
    match accepted_item_assignments(assignments) {
        Ok(accepted) => planned_item(&accepted).map_err(|failure| vec![failure.into()]),
        Err(rejected) => {
            let missing: Vec<String> = ItemField::ALL
                .into_iter()
                .filter(|field| field.required())
                .filter(|field| {
                    !assignments
                        .entries
                        .iter()
                        .any(|entry| entry.field == field.name())
                })
                .map(|field| field.name().to_owned())
                .collect();
            let mut faults = vec![Failure::InvalidAssignments(rejected)];
            if !missing.is_empty() {
                faults.push(Failure::MissingRequiredItemFacts { fields: missing });
            }
            Err(faults)
        }
    }
}

/// One accepted Item assignment: a candidate value, or the explicit clearing of an
/// optional Item fact.
enum ItemValue {
    Clear,
    Set(serde_json::Value),
}

/// The assignments of one Item set that broke no rule, or every one that did.
fn accepted_item_assignments(
    assignments: &Assignments,
) -> Result<Vec<(ItemField, ItemValue)>, Vec<RejectedAssignment>> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(ItemField, ItemValue)>::new();
    for Assignment {
        field,
        value,
        confirmed,
    } in &assignments.entries
    {
        let duplicate = accepted
            .iter()
            .any(|(accepted, _)| accepted.name() == field);
        match item_assignment(field, value, duplicate, *confirmed) {
            Ok(value) => accepted.push((
                ItemField::named(field).expect("an accepted Item field is one of its fields"),
                value,
            )),
            Err(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
        }
    }
    if rejected.is_empty() {
        Ok(accepted)
    } else {
        Err(rejected)
    }
}

/// Accepts one Item assignment as the exact candidate value it will store, so an
/// accepted numeric value is one the durable record can already hold.
fn item_assignment(
    field: &str,
    value: &str,
    duplicate: bool,
    confirmed: bool,
) -> Result<ItemValue, RejectedAssignmentReason> {
    let Some(field) = ItemField::named(field) else {
        return Err(RejectedAssignmentReason::UnknownField);
    };
    if duplicate {
        return Err(RejectedAssignmentReason::Duplicate);
    }
    // No Item fact is a Reusable Free-Entry Value, so none takes a confirmation.
    if confirmed {
        return Err(RejectedAssignmentReason::NotAReusableValue);
    }
    let value = value.trim();
    if value.is_empty() {
        return if field.required() {
            Err(RejectedAssignmentReason::BlankRequiredValue)
        } else {
            Ok(ItemValue::Clear)
        };
    }
    match field {
        // Quantity and Unit Price accept any contract-valid zero or negative value,
        // and refuse text the durable record could not hold as a finite number.
        ItemField::Quantity | ItemField::UnitPrice => item_number(value)
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number)
            .map(ItemValue::Set)
            .ok_or(RejectedAssignmentReason::NotAFiniteNumber),
        _ => Ok(ItemValue::Set(serde_json::Value::String(value.to_owned()))),
    }
}

/// Why one assembled Order Item is not one the record can hold.
enum PlannedItemFailure {
    MissingRequiredFacts { fields: Vec<String> },
    Invalid,
}

fn planned_item(assignments: &[(ItemField, ItemValue)]) -> Result<OrderItem, PlannedItemFailure> {
    let mut object = serde_json::Map::new();
    for (field, value) in assignments {
        match value {
            ItemValue::Clear => {
                object.remove(field.name());
            }
            ItemValue::Set(value) => {
                object.insert(field.name().to_owned(), value.clone());
            }
        }
    }
    let missing = ItemField::ALL
        .into_iter()
        .filter(|field| field.required() && !object.contains_key(field.name()))
        .map(|field| field.name().to_owned())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(PlannedItemFailure::MissingRequiredFacts { fields: missing });
    }
    serde_json::from_value::<OrderItem>(serde_json::Value::Object(object))
        .map_err(|_| PlannedItemFailure::Invalid)
}

impl From<PlannedItemFailure> for Failure {
    fn from(failure: PlannedItemFailure) -> Self {
        match failure {
            PlannedItemFailure::MissingRequiredFacts { fields } => {
                Self::MissingRequiredItemFacts { fields }
            }
            PlannedItemFailure::Invalid => Self::InvalidFacts,
        }
    }
}

/// The refusal a creation or an adoption answers for every fault resolving its facts met:
/// the one fault itself, or every one together, as an edit names them.
pub(crate) fn refusal_of(mut faults: Vec<DraftFailure>) -> Failure {
    if faults.len() == 1 {
        faults.remove(0).failure
    } else {
        Failure::InvalidDraft(faults)
    }
}
