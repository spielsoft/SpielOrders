//! Drafts: an editor's whole Order, Vendor, or file text, holding whatever the user typed,
//! and the rules a draft applies as the user types, before it is planned.
//!
//! A draft never rejects or normalizes what was typed; value errors come from planning it,
//! which applies every rule again. The live rules here only keep what an editor shows the
//! answer the Package would give: a Status entering Ordered takes today's date, and each
//! Item's and the Order's Total follow the numbers typed.
//!
//! Decision sources: Prototype the SpielOrders desktop experience; Define the Rust CLI
//! capability contract.

use crate::{
    Assignments, Change, Coded, OrderRef,
    metadata::{OrderDocument, OrderStatus, Vendor},
    order::{item_total, order_total},
    order_facts::{
        ItemField, OrderCreationDefaults, OrderField, date_ordered_after_status_change,
        item_number, parse_status,
    },
    ports::LocalDate,
    vendors::{ContactField, VendorField, vendor_names_match},
};
use std::path::PathBuf;

/// What a draft holds: an Order with its Items, a Vendor with its Contacts, or a file's
/// text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftKind {
    Order,
    Vendor,
    Text,
}

impl DraftKind {
    /// The facts a draft of this kind holds, in the order an editor lists them.
    fn facts(self) -> Vec<Fact> {
        match self {
            Self::Order => OrderField::ALL.into_iter().map(Fact::Order).collect(),
            Self::Vendor => VendorField::ALL.into_iter().map(Fact::Vendor).collect(),
            Self::Text => vec![Fact::Content],
        }
    }

    /// The facts each row of a draft of this kind holds, in the order an editor lists
    /// them.
    pub(crate) fn row_facts(self) -> Vec<RowFact> {
        match self {
            Self::Order => ItemField::ALL.into_iter().map(RowFact::Item).collect(),
            Self::Vendor => ContactField::ALL
                .into_iter()
                .map(RowFact::Contact)
                .collect(),
            Self::Text => Vec::new(),
        }
    }
}

/// One fact of a drafted record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Fact {
    Order(OrderField),
    Vendor(VendorField),
    /// The whole text of a file.
    Content,
}

impl Fact {
    /// The name the record's assignments give this fact.
    fn name(self) -> &'static str {
        match self {
            Self::Order(field) => field.name(),
            Self::Vendor(field) => field.name(),
            Self::Content => "content",
        }
    }
}

/// One fact of a drafted row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RowFact {
    Item(ItemField),
    Contact(ContactField),
}

impl RowFact {
    fn name(self) -> &'static str {
        match self {
            Self::Item(field) => field.name(),
            Self::Contact(field) => field.name(),
        }
    }
}

/// What a draft saves as when it is planned.
#[derive(Clone, Debug)]
pub(crate) enum Origin {
    NewOrder {
        root: PathBuf,
    },
    EditOrder {
        directory: PathBuf,
        original: Box<OrderDocument>,
    },
    Adoption {
        root: PathBuf,
        directory: PathBuf,
    },
    NewVendor {
        root: PathBuf,
    },
    EditVendor {
        root: PathBuf,
        original: Box<Vendor>,
    },
    /// A diagnosis draft, which only its diagnosis plans.
    Repair,
}

impl Origin {
    /// Whether the draft gives a record its first identity.
    pub(crate) fn is_new_record(&self) -> bool {
        matches!(
            self,
            Self::NewOrder { .. } | Self::NewVendor { .. } | Self::Adoption { .. }
        )
    }
}

/// Why a draft refuses a change: the change names a fact, a row, or a confirmation the
/// draft does not have.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftRefusal {
    /// A fact the draft or its rows do not hold, or rows a draft of its kind cannot hold.
    NotItsField,
    /// A row position the draft does not have.
    NoRow,
}

/// One row of a draft: an Item of an Order, or a Contact of a Vendor.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    /// The row of the read record this row began as, if it did.
    pub(crate) origin: Option<usize>,
    facts: Vec<(RowFact, String)>,
}

impl Row {
    /// The text `fact` holds, or "".
    pub(crate) fn fact(&self, fact: RowFact) -> &str {
        value(&self.facts, fact)
    }

    /// Every fact this row holds, in the order an editor lists them.
    pub(crate) fn facts(&self) -> &[(RowFact, String)] {
        &self.facts
    }
}

/// What a draft held when it was made: the read it edits, or nothing for a new record.
#[derive(Clone, Debug, Default)]
pub(crate) struct Initial {
    pub(crate) facts: Vec<(Fact, String)>,
    pub(crate) rows: Vec<Vec<(RowFact, String)>>,
}

/// An editor's draft of one Order, one Vendor, or one file's text.
#[derive(Clone, Debug)]
pub(crate) struct Draft {
    kind: DraftKind,
    origin: Origin,
    facts: Vec<(Fact, String)>,
    rows: Vec<Row>,
    /// The values to be added to the Package's values where it lacks them.
    confirmed: Vec<OrderField>,
    /// The facts set since the draft was made, including to the value they had.
    touched: Vec<Fact>,
    initial: Initial,
}

impl Draft {
    /// A draft of kind `kind` holding `facts` and `rows`; a fact or row fact its kind does
    /// not hold is dropped, and one it holds but was not given is empty. A draft of an
    /// existing record remembers what it began as.
    pub(crate) fn new(
        kind: DraftKind,
        origin: Origin,
        facts: &[(Fact, String)],
        rows: Vec<Vec<(RowFact, String)>>,
    ) -> Self {
        let row_facts = kind.row_facts();
        let facts = kind
            .facts()
            .into_iter()
            .map(|fact| (fact, value(facts, fact).to_owned()))
            .collect::<Vec<_>>();
        let rows = rows
            .into_iter()
            .enumerate()
            .map(|(index, mut facts)| {
                facts.retain(|(fact, _)| row_facts.contains(fact));
                Row {
                    origin: Some(index),
                    facts,
                }
            })
            .collect::<Vec<_>>();
        let initial = if origin.is_new_record() {
            Initial::default()
        } else {
            Initial {
                facts: facts.clone(),
                rows: rows.iter().map(|row| row.facts.clone()).collect(),
            }
        };
        Self {
            kind,
            origin,
            facts,
            rows,
            confirmed: Vec::new(),
            touched: Vec::new(),
            initial,
        }
    }

    /// A draft of a new Order in the Package at `root`, holding its Title, Date Ordered,
    /// and Status defaults.
    pub(crate) fn new_order(root: PathBuf, today: LocalDate) -> Self {
        let defaults = OrderCreationDefaults::new(&|| today);
        Self::new(
            DraftKind::Order,
            Origin::NewOrder { root },
            &[
                (Fact::Order(OrderField::Title), NEW_ORDER_TITLE.to_owned()),
                (Fact::Order(OrderField::DateOrdered), defaults.date_ordered),
                (
                    Fact::Order(OrderField::Status),
                    defaults.status.code().to_owned(),
                ),
            ],
            Vec::new(),
        )
    }

    /// A draft of a new Vendor beside `vendors`, holding a Name none of them uses.
    pub(crate) fn new_vendor(root: PathBuf, vendors: &[Vendor]) -> Self {
        Self::new(
            DraftKind::Vendor,
            Origin::NewVendor { root },
            &[(Fact::Vendor(VendorField::Name), new_vendor_name(vendors))],
            Vec::new(),
        )
    }

    pub(crate) fn kind(&self) -> DraftKind {
        self.kind
    }

    pub(crate) fn origin(&self) -> &Origin {
        &self.origin
    }

    /// Every fact this draft holds, in the order an editor lists them.
    pub(crate) fn facts(&self) -> &[(Fact, String)] {
        &self.facts
    }

    /// The text `fact` holds, or "" for a fact this draft does not hold.
    pub(crate) fn fact(&self, fact: Fact) -> &str {
        value(&self.facts, fact)
    }

    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The record this draft began from, as its facts and rows.
    pub(crate) fn initial(&self) -> &Initial {
        &self.initial
    }

    /// Sets `fact` to `text` exactly as typed, and applies the live rule: choosing Ordered
    /// as the Status takes today's Date Ordered, except in an adoption, whose facts are
    /// all the user's. Setting a value withdraws its confirmation.
    pub(crate) fn set(
        &mut self,
        fact: Fact,
        text: &str,
        today: LocalDate,
    ) -> Result<(), DraftRefusal> {
        if !self.facts.iter().any(|(each, _)| *each == fact) {
            return Err(DraftRefusal::NotItsField);
        }
        let date = Fact::Order(OrderField::DateOrdered);
        if fact == Fact::Order(OrderField::Status)
            && !matches!(self.origin, Origin::Adoption { .. })
        {
            let moved = draft_date_ordered(self.fact(fact), text, self.fact(date), today);
            self.put(date, moved);
        }
        if let Fact::Order(field) = fact {
            self.confirmed.retain(|confirmed| *confirmed != field);
        }
        self.put(fact, text.to_owned());
        Ok(())
    }

    fn put(&mut self, fact: Fact, text: String) {
        if let Some((_, value)) = self.facts.iter_mut().find(|(each, _)| *each == fact) {
            *value = text;
        }
        if !self.touched.contains(&fact) {
            self.touched.push(fact);
        }
    }

    /// Inserts an empty row before row `at`; `at` equal to the row count appends.
    pub(crate) fn insert_row(&mut self, at: usize) -> Result<(), DraftRefusal> {
        if matches!(self.origin, Origin::Repair) || self.kind == DraftKind::Text {
            return Err(DraftRefusal::NotItsField);
        }
        if at > self.rows.len() {
            return Err(DraftRefusal::NoRow);
        }
        let facts = self
            .kind
            .row_facts()
            .into_iter()
            .map(|fact| (fact, String::new()))
            .collect();
        self.rows.insert(
            at,
            Row {
                origin: None,
                facts,
            },
        );
        Ok(())
    }

    /// Removes row `at`.
    pub(crate) fn remove_row(&mut self, at: usize) -> Result<(), DraftRefusal> {
        if at >= self.rows.len() {
            return Err(DraftRefusal::NoRow);
        }
        self.rows.remove(at);
        Ok(())
    }

    /// Moves row `from` so that it becomes row `to`.
    pub(crate) fn move_row(&mut self, from: usize, to: usize) -> Result<(), DraftRefusal> {
        if from >= self.rows.len() || to >= self.rows.len() {
            return Err(DraftRefusal::NoRow);
        }
        let moved = self.rows.remove(from);
        self.rows.insert(to, moved);
        Ok(())
    }

    /// Sets `fact` of row `at` to `text` exactly as typed.
    pub(crate) fn set_row(
        &mut self,
        at: usize,
        fact: RowFact,
        text: &str,
    ) -> Result<(), DraftRefusal> {
        if !self.kind.row_facts().contains(&fact) {
            return Err(DraftRefusal::NotItsField);
        }
        let row = self.rows.get_mut(at).ok_or(DraftRefusal::NoRow)?;
        match row.facts.iter_mut().find(|(each, _)| *each == fact) {
            Some((_, value)) => *value = text.to_owned(),
            None => row.facts.push((fact, text.to_owned())),
        }
        Ok(())
    }

    /// Asks that saving add the value `field` holds to the Package's values where the
    /// Package lacks it. Only an Order's four value fields take a confirmation.
    pub(crate) fn confirm(&mut self, field: OrderField) -> Result<(), DraftRefusal> {
        if self.kind != DraftKind::Order || field.reusable().is_none() {
            return Err(DraftRefusal::NotItsField);
        }
        if !self.confirmed.contains(&field) {
            self.confirmed.push(field);
        }
        Ok(())
    }

    /// Row `at`'s live Item Total, for an Order draft: `None` where its numbers do not
    /// parse or their product is not finite.
    pub(crate) fn item_total(&self, row: &Row) -> Option<f64> {
        draft_item_total(
            row.fact(RowFact::Item(ItemField::Quantity)),
            row.fact(RowFact::Item(ItemField::UnitPrice)),
        )
    }

    /// The live Order Total of an Order draft: `None` where any row has no Item Total.
    pub(crate) fn order_total(&self) -> Option<f64> {
        let totals = self
            .rows
            .iter()
            .map(|row| self.item_total(row))
            .collect::<Vec<_>>();
        draft_order_total(&totals)
    }

    /// The facts and rows as the assignments a plan takes. An existing Order's draft
    /// names only the facts that were set, so an edit can tell a fact named again from one
    /// left alone; the rest stay as the record has them.
    pub(crate) fn assignments(&self) -> (Assignments, Vec<Assignments>) {
        let existing = matches!(self.origin, Origin::EditOrder { .. });
        let mut facts = Assignments::new();
        for (fact, value) in &self.facts {
            if existing && !self.touched.contains(fact) {
                continue;
            }
            let confirmed = matches!(fact, Fact::Order(field) if self.confirmed.contains(field));
            facts = if confirmed {
                facts.introduce(fact.name(), value.clone())
            } else {
                facts.assign(fact.name(), value.clone())
            };
        }
        let rows = self
            .rows
            .iter()
            .map(|row| {
                row.facts
                    .iter()
                    .fold(Assignments::new(), |row, (fact, value)| {
                        row.assign(fact.name(), value.clone())
                    })
            })
            .collect();
        (facts, rows)
    }

    /// The change this draft saves as, or `None` for a diagnosis draft.
    pub(crate) fn change(&self) -> Option<Change> {
        let (assignments, rows) = self.assignments();
        Some(match &self.origin {
            Origin::NewOrder { root } => Change::CreateOrder {
                root: root.clone(),
                assignments,
                items: rows,
            },
            Origin::EditOrder {
                directory,
                original,
            } => Change::SaveOrder {
                order: OrderRef::Directory(directory.clone()),
                assignments,
                items: rows,
                original: (**original).clone(),
            },
            Origin::Adoption { root, directory } => Change::AdoptOrder {
                root: root.clone(),
                directory: directory.clone(),
                assignments,
                items: rows,
            },
            Origin::NewVendor { root } => Change::CreateVendor {
                root: root.clone(),
                assignments,
                contacts: rows,
            },
            Origin::EditVendor { root, original } => Change::SaveVendor {
                root: root.clone(),
                assignments,
                contacts: rows,
                original: (**original).clone(),
            },
            Origin::Repair => return None,
        })
    }
}

/// The text `fact` holds among `facts`, or "".
fn value<F: PartialEq>(facts: &[(F, String)], fact: F) -> &str {
    facts
        .iter()
        .find(|(each, _)| *each == fact)
        .map_or("", |(_, value)| value.as_str())
}

/// The Title a new Order's draft starts with.
const NEW_ORDER_TITLE: &str = "New Order";

/// The Name a new Vendor's draft starts with: `New Vendor`, numbered from 2 past every
/// Name the Package already uses, since Vendor Names are unique within it.
pub(crate) fn new_vendor_name(vendors: &[Vendor]) -> String {
    (1..)
        .map(|number| match number {
            1 => "New Vendor".to_owned(),
            _ => format!("New Vendor {number}"),
        })
        .find(|name| {
            !vendors
                .iter()
                .any(|vendor| vendor_names_match(name, vendor.name.as_str()))
        })
        .expect("a Package holds finitely many Vendors")
}

/// The Date Ordered a draft holds once its Status changes from `previous` to `next`:
/// entering Ordered takes today's date, which the user may then change.
fn draft_date_ordered(previous: &str, next: &str, current: &str, today: LocalDate) -> String {
    match parse_status(next) {
        Some(next) => {
            let previous = parse_status(previous).unwrap_or(OrderStatus::InPreparation);
            date_ordered_after_status_change(previous, next, current, &|| today)
        }
        None => current.to_owned(),
    }
}

/// One draft row's Item Total, or `None` where its numbers do not parse or their
/// product is not finite.
fn draft_item_total(quantity: &str, unit_price: &str) -> Option<f64> {
    Some(item_total(item_number(quantity)?, item_number(unit_price)?))
        .filter(|total| total.is_finite())
}

/// A draft's Order Total, or `None` where any row has no Item Total.
fn draft_order_total(item_totals: &[Option<f64>]) -> Option<f64> {
    let totals: Option<Vec<f64>> = item_totals.iter().copied().collect();
    Some(order_total(totals?.into_iter())).filter(|total| total.is_finite())
}
