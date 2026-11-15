//! Which Orders and Vendors of a loaded Package match a query.
//!
//! **A loaded Package rather than a path, so nothing here reads.** Every fact a search
//! matches on is already in the projection: the canonical Order records, the Vendor
//! record, and each healthy Order's README text, which the ordinary load reads with the
//! Order. A capability that took a root would load the Package a second time, and a
//! caller filtering a list it is already holding would be searching a different Package
//! from the one it is showing.
//!
//! **Visibility, never order.** A search removes the rows that do not match from the
//! Package's own list, so the load's ordering survives.
//!
//! Decision sources: Define lookup and search behavior.

use crate::code::Coded;
use crate::{
    Answer,
    canonical::{DamagedOrder, HealthyOrder, Package, ReusableValueProjection},
    metadata::{Currency, Vendor, VendorId},
};
use icu_casemap::CaseMapper;
use pulldown_cmark::{Event, Options, Parser, TagEnd};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// Whether a query distinguishes letter case.
///
/// `Default` is the decision's default, so a caller that asks for nothing gets it. It
/// reaches exactly one place, the fold every term and fact goes through, which is what
/// keeps the control from becoming a second matching mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaseSensitivity {
    #[default]
    Insensitive,
    Sensitive,
}

/// One fact an Order search can be confined to. The Vendor is matched by its Name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderFact {
    Title,
    DateOrdered,
    Status,
    Vendor,
    Orderer,
    OrderingInstitution,
    Project,
    OrderGroup,
    Currency,
    OrderId,
    Description,
    CatalogNumber,
    Quantity,
    UnitPrice,
    Notes,
}

/// One fact a Vendor search can be confined to. A Contact fact matches any Contact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VendorFact {
    Name,
    Phone,
    Email,
    Website,
    Address,
    PurchasingNotes,
    VendorId,
    ContactName,
    ContactRole,
    ContactPhone,
    ContactEmail,
}

/// One parsed query.
///
/// The terms are folded once here rather than at each fact they are compared with, so one
/// query asked of a whole Package folds each term exactly once. A query searches every
/// searchable fact unless it is confined to one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchQuery {
    terms: Vec<String>,
    case: CaseSensitivity,
    order_fact: Option<OrderFact>,
    vendor_fact: Option<VendorFact>,
}

impl SearchQuery {
    /// Parses `text` into the terms it spells.
    ///
    /// Whitespace separates terms and belongs to none of them, so no term can contain any
    /// — which is what lets facts be told apart by whitespace without a term ever
    /// straddling the join.
    pub fn new(text: &str, case: CaseSensitivity) -> Self {
        Self {
            terms: text
                .split_whitespace()
                .map(|term| folded(term, case))
                .collect(),
            case,
            order_fact: None,
            vendor_fact: None,
        }
    }

    /// This query confined to one fact of an Order.
    pub fn in_order_fact(self, fact: OrderFact) -> Self {
        Self {
            order_fact: Some(fact),
            ..self
        }
    }

    /// This query confined to one fact of a Vendor.
    pub fn in_vendor_fact(self, fact: VendorFact) -> Self {
        Self {
            vendor_fact: Some(fact),
            ..self
        }
    }

    fn order_wants(&self, fact: OrderFact) -> bool {
        self.order_fact.is_none_or(|only| only == fact)
    }

    fn vendor_wants(&self, fact: VendorFact) -> bool {
        self.vendor_fact.is_none_or(|only| only == fact)
    }

    /// Whether parsing found no terms, so this query applies no text restriction.
    pub(crate) fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// An empty collection of facts, folded the way this query is.
    ///
    /// The query builds it so that the two cannot come to disagree about case: there is
    /// one [`CaseSensitivity`] in play and this is the only place it is handed on.
    fn facts(&self) -> Facts {
        Facts {
            case: self.case,
            folded: Vec::new(),
        }
    }

    /// Whether every term of this query lands on some fact of one entity.
    ///
    /// Terms are independent, so each may land on a different fact and an entity carrying
    /// all of them is retained once however many facts they landed on. A query with no
    /// terms lands vacuously, which is how an empty query restricts nothing.
    fn retains(&self, facts: &Facts) -> bool {
        self.terms.iter().all(|term| facts.carry(term))
    }
}

/// A row of the ordinary Order list: a healthy Order at its position among
/// [`crate::canonical::CurrentPackageCandidate::healthy_orders`], or a Damaged Order's placeholder.
#[derive(Clone, Debug, PartialEq)]
pub enum OrderListEntry {
    Healthy {
        position: usize,
        order: Box<HealthyOrder>,
    },
    Damaged(Box<DamagedOrder>),
}

/// Whether the ordinary Vendor list includes Hidden Vendors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VendorVisibility {
    Offered,
    All,
}

/// One of the four Reusable Free-Entry Value categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReusableValueKind {
    OrderingInstitution,
    Orderer,
    Project,
    OrderGroup,
}

/// Every list a caller reads off an opened Package answers with that Package's findings
/// beside it, as a read of the Package does.
impl Package {
    /// The ordinary Order list under `query`, in the Package's own order.
    ///
    /// A healthy row keeps its position among the healthy Orders, so a caller resolves its
    /// Vendor with [`crate::canonical::CurrentPackageCandidate::vendor_for_order`]. A Damaged Order has no
    /// facts to match on, so its placeholder is listed only under an empty query.
    pub fn orders_matching(&self, query: &SearchQuery) -> Answer<Vec<OrderListEntry>> {
        self.orders_matching_or_holding(query, &[])
    }

    /// The ordinary Order list of the healthy Orders matching `query` or whose Order
    /// Directory is one of `holding`, where Supporting Material matched; in the Package's
    /// own order, each once. As in [`Self::orders_matching`], Damaged Order placeholders
    /// are listed only under an empty query.
    pub fn orders_matching_or_holding(
        &self,
        query: &SearchQuery,
        holding: &[std::path::PathBuf],
    ) -> Answer<Vec<OrderListEntry>> {
        let package = &self.contents;
        let mut rows: Vec<_> = package
            .healthy_orders
            .iter()
            .enumerate()
            .filter(|(position, order)| {
                holding.contains(&order.paths.directory.absolute_path)
                    || query.retains(&order_facts(
                        order,
                        package.vendor_for_order(*position),
                        query,
                    ))
            })
            .map(|(position, order)| OrderListEntry::Healthy {
                position,
                order: Box::new(order.clone()),
            })
            .collect();
        if query.is_empty() {
            rows.extend(
                package
                    .damaged_orders
                    .iter()
                    .cloned()
                    .map(Box::new)
                    .map(OrderListEntry::Damaged),
            );
        }
        package.answer(rows)
    }

    /// The Vendors matching `query` that `visibility` lists, in the Package's own order.
    pub fn vendors_matching(
        &self,
        query: &SearchQuery,
        visibility: VendorVisibility,
    ) -> Answer<Vec<Vendor>> {
        let package = &self.contents;
        package.answer(
            package
                .vendors
                .iter()
                .filter(|vendor| visibility == VendorVisibility::All || !vendor.is_hidden())
                .filter(|vendor| query.retains(&vendor_facts(vendor, query)))
                .cloned()
                .collect(),
        )
    }

    /// The current values of one Reusable Free-Entry Value category.
    pub fn values(&self, kind: ReusableValueKind) -> Answer<Vec<ReusableValueProjection>> {
        let package = &self.contents;
        package.answer(
            match kind {
                ReusableValueKind::OrderingInstitution => {
                    &package.projections.ordering_institutions
                }
                ReusableValueKind::Orderer => &package.projections.orderers,
                ReusableValueKind::Project => &package.projections.projects,
                ReusableValueKind::OrderGroup => &package.projections.order_groups,
            }
            .clone(),
        )
    }
}

/// Everything one entity is matched on, each fact folded once and kept apart from its
/// neighbours.
///
/// Apart rather than run together, because a term is a substring of *a fact*: a single
/// concatenated haystack would let a term straddle the join between two facts and report a
/// match neither of them carries.
struct Facts {
    case: CaseSensitivity,
    folded: Vec<String>,
}

impl Facts {
    fn add(&mut self, text: &str) {
        self.folded.push(folded(text, self.case));
    }

    fn add_optional(&mut self, text: Option<&str>) {
        if let Some(text) = text {
            self.add(text);
        }
    }

    fn add_if(&mut self, wanted: bool, text: &str) {
        if wanted {
            self.add(text);
        }
    }

    fn add_optional_if(&mut self, wanted: bool, text: Option<&str>) {
        if wanted {
            self.add_optional(text);
        }
    }

    /// One stored number, as the shortest decimal that reads back as the same number.
    ///
    /// A quantity's only spelling, which Define lookup and search behavior is where the
    /// reason lives.
    fn add_stored_number(&mut self, value: f64) {
        self.add(&value.to_string());
    }

    /// One monetary fact, by the spelling its record stores and by its spelling rounded to
    /// two decimal places, both of which Define lookup and search behavior matches.
    ///
    /// Unit Price is the only monetary fact reached from here. The Item Total and the
    /// Order Total are calculated, and a calculated total is one of the exclusions.
    fn add_monetary_number(&mut self, value: f64) {
        self.add_stored_number(value);
        // Adding a positive zero turns a negative zero positive and changes nothing else,
        // so no zero is spelled with a sign it does not carry.
        self.add(&format!("{:.2}", value + 0.0));
    }

    /// One Markdown document's text, with the markup it is written in left out.
    ///
    /// Parsed rather than matched as written, because raw Markdown syntax is one of the
    /// decision's exclusions — and the one that would otherwise readmit others, since a
    /// link's destination is a filename or a path and both are excluded in their own
    /// right. A parser rather than a hand-written stripper, for the same reason the
    /// diacritic fold is not hand-written.
    fn add_markdown(&mut self, markdown: Option<&str>) {
        self.add_optional(markdown.map(document_text).as_deref());
    }

    fn carry(&self, term: &str) -> bool {
        self.folded.iter().any(|fact| fact.contains(term))
    }
}

/// The text of one Markdown document: its text runs, with a break at every block
/// boundary.
///
/// The break is what keeps the end of one block and the start of the next from reading as
/// one word — a nested list, where a block opens between two words without either of them
/// ending one, is why an opening boundary counts as much as a closing one. An inline
/// element is neither, because a word an emphasis interrupts is still one word.
///
/// The extensions below are a choice: which of them counts as syntax rather than text is
/// nothing the decision says, and these are the ones an ordinary README is written with.
fn document_text(markdown: &str) -> String {
    let extensions = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut text = String::new();
    for event in Parser::new_ext(markdown, extensions) {
        match event {
            Event::Text(run) | Event::Code(run) => text.push_str(&run),
            // A raw HTML run is a fragment of tags around character data, and the data is
            // the README's text like any other. Its tags carry a link's destination and an
            // image's source, both excluded in their own right, so they are what is
            // dropped.
            Event::Html(run) | Event::InlineHtml(run) => text.push_str(&html_text(&run)),
            Event::SoftBreak | Event::HardBreak | Event::Rule => text.push('\n'),
            Event::Start(tag) => {
                if is_a_block(TagEnd::from(tag)) {
                    text.push('\n');
                }
            }
            Event::End(tag) if is_a_block(tag) => text.push('\n'),
            // What is left is markup that spells no text of its own: a footnote marker, a
            // task-list checkbox.
            _ => {}
        }
    }
    text
}

/// The character data of one raw HTML fragment: everything outside its tags.
///
/// **Not an HTML parser, and not one in hiding.** A fragment is not a document, and this
/// answers one question about it: which characters a reader would see. A `<` opens a tag
/// and the first `>` outside a quoted attribute value closes it — which is enough to keep
/// every attribute out, and attributes are the reason this is not simply matched as
/// written.
fn html_text(html: &str) -> String {
    let mut text = String::new();
    let mut quote = None;
    let mut inside_tag = false;
    for character in html.chars() {
        match (inside_tag, quote, character) {
            (false, _, '<') => inside_tag = true,
            (false, _, _) => text.push(character),
            (true, Some(open), _) if character == open => quote = None,
            (true, None, '"' | '\'') => quote = Some(character),
            (true, None, '>') => inside_tag = false,
            (true, _, _) => {}
        }
    }
    text
}

/// Whether this element is a block, so text on either side of its boundary belongs to a
/// different one.
///
/// Spelled as the inline exceptions because that is the short list, and because it is the
/// safe way round: an element this build does not recognize is treated as a block, which
/// separates text that might have belonged together rather than joining text that never
/// did.
fn is_a_block(tag: TagEnd) -> bool {
    !matches!(
        tag,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::Image
    )
}

/// One Order's searchable facts, together with the current shared facts of the Vendor it
/// names.
///
/// The Vendor's facts are read live from the Package's Vendor record rather than from
/// anything the Order stores, which is what makes editing a Vendor change which Orders
/// match without touching those Orders.
///
/// What the record carries and this leaves out is what the decision excludes: the
/// technical locator, which is a filesystem path, and the schema version. So are the
/// Directory and the documents around the record, and the Item Totals and Order Total
/// calculated from the Items.
fn order_facts(order: &HealthyOrder, vendor: Option<&Vendor>, query: &SearchQuery) -> Facts {
    use OrderFact as Fact;
    let document = &order.document;
    let wants = |fact| query.order_wants(fact);
    let everything = query.order_fact.is_none();
    let mut facts = query.facts();
    facts.add_if(wants(Fact::OrderId), document.order_id.as_str());
    facts.add_if(wants(Fact::Title), document.title.as_str());
    facts.add_if(wants(Fact::DateOrdered), document.date_ordered.as_str());
    facts.add_if(wants(Fact::Status), document.status.code());
    // The Vendor Reference the record stores: a canonical Order fact like any other, and
    // named in none of the decision's exclusions, so one Vendor ID reaches the Vendor and
    // the Orders that name it alike.
    facts.add_optional_if(
        everything,
        document.vendor_id.as_ref().map(VendorId::as_str),
    );
    facts.add_optional_if(wants(Fact::Orderer), document.orderer.as_deref());
    facts.add_optional_if(
        wants(Fact::OrderingInstitution),
        document.ordering_institution.as_deref(),
    );
    facts.add_optional_if(
        wants(Fact::Currency),
        document.currency.as_ref().map(Currency::as_str),
    );
    facts.add_optional_if(wants(Fact::Project), document.project.as_deref());
    facts.add_optional_if(wants(Fact::OrderGroup), document.order_group.as_deref());
    for item in &document.items {
        facts.add_if(wants(Fact::Description), item.description.as_str());
        facts.add_optional_if(wants(Fact::CatalogNumber), item.catalog_number.as_deref());
        if wants(Fact::Quantity) {
            facts.add_stored_number(item.quantity.get());
        }
        if wants(Fact::UnitPrice) {
            facts.add_monetary_number(item.unit_price.get());
        }
        facts.add_optional_if(wants(Fact::Notes), item.notes.as_deref());
    }
    // The README the load established for this Order. `None` is a document that exists and
    // could not be read, so there is no text to match on and none is invented — the load's
    // diagnostics are what say why. It is no single fact, so only a whole search reaches it.
    if everything {
        facts.add_markdown(order.readme_text.as_deref());
    }
    match (vendor, query.order_fact) {
        (Some(vendor), None) => add_shared_vendor_facts(&mut facts, vendor, query),
        (Some(vendor), Some(Fact::Vendor)) => facts.add(vendor.name.as_str()),
        _ => {}
    }
    facts
}

/// One Vendor's searchable facts: its identity and its shared facts.
fn vendor_facts(vendor: &Vendor, query: &SearchQuery) -> Facts {
    let mut facts = query.facts();
    facts.add_if(
        query.vendor_wants(VendorFact::VendorId),
        vendor.vendor_id.as_str(),
    );
    add_shared_vendor_facts(&mut facts, vendor, query);
    facts
}

/// Every current shared fact of one Vendor, Contacts included.
///
/// Visibility is not among them. **A choice, not a rule:** a Hidden Vendor is in a state
/// rather than carrying a fact, and this core spells no text for that state, so there is
/// nothing here for a term to be a substring of.
/// An Order search reaches them all or, confined to its Vendor, only the Name.
fn add_shared_vendor_facts(facts: &mut Facts, vendor: &Vendor, query: &SearchQuery) {
    use VendorFact as Fact;
    let wants = |fact| query.vendor_wants(fact);
    facts.add_if(wants(Fact::Name), vendor.name.as_str());
    facts.add_optional_if(wants(Fact::Phone), vendor.phone.as_deref());
    facts.add_optional_if(wants(Fact::Email), vendor.email.as_deref());
    facts.add_optional_if(wants(Fact::Website), vendor.website.as_deref());
    facts.add_optional_if(wants(Fact::Address), vendor.address.as_deref());
    facts.add_optional_if(
        wants(Fact::PurchasingNotes),
        vendor.purchasing_notes.as_deref(),
    );
    for contact in &vendor.contacts {
        facts.add_if(wants(Fact::ContactName), contact.name.as_str());
        facts.add_optional_if(wants(Fact::ContactRole), contact.role.as_deref());
        facts.add_optional_if(wants(Fact::ContactPhone), contact.phone.as_deref());
        facts.add_optional_if(wants(Fact::ContactEmail), contact.email.as_deref());
    }
}

/// One text as this search compares it: decomposed, stripped of its combining marks, and
/// case-folded where the query asks for no case.
///
/// Canonical decomposition folds a diacritic away without transliterating anything: `é`
/// decomposes into `e` and a combining acute, and dropping the mark leaves the letter. It
/// settles a text that arrives already decomposed too, because both spellings decompose to
/// the same thing. A letter whose mark is part of the letter does not decompose and is
/// therefore not folded: `ø`, `ł` and `đ` stay as they are.
///
/// ICU supplies a folded string, allowing substring matching across expanding folds as
/// well as context-independent forms of the same letter.
fn folded(text: &str, case: CaseSensitivity) -> String {
    let letters: String = text
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .collect();
    match case {
        CaseSensitivity::Insensitive => CaseMapper::new().fold_string(&letters).into_owned(),
        CaseSensitivity::Sensitive => letters,
    }
}
