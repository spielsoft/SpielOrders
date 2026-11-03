//! Which Orders and Vendors of a loaded Package match a query.
//!
//! **A loaded Package rather than a path, so nothing here reads.** Every fact a search
//! matches on is already in the projection: the canonical Order records, the Vendor
//! record, and each healthy Order's README text, which the ordinary load reads with the
//! Order. A capability that took a root would load the Package a second time, and a
//! caller filtering a list it is already holding would be searching a different Package
//! from the one it is showing.
//!
//! **Visibility, never order.** [`matching_orders`] hands back the same `(position,
//! Order)` pairs `healthy_orders().iter().enumerate()` yields, with the Orders that do not
//! match removed, so the load's ordering survives and a caller resolves each Order's
//! Vendor through [`CurrentPackageCandidate::vendor_for_order`] exactly as it does for the
//! whole list.
//!
//! Decision sources: Define lookup and search behavior.

use crate::{
    canonical::{CurrentPackageCandidate, HealthyOrder},
    metadata::{Currency, Vendor, VendorId},
    money::two_decimal_spelling,
};
use icu_casemap::CaseMapper;
use pulldown_cmark::{Event, Options, Parser, TagEnd};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// Whether a query distinguishes letter case.
///
/// `Default` is the decision's default, so a caller that asks for nothing gets it. It
/// reaches exactly one place — [`folded`] — which is what keeps the control from becoming
/// a second matching mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaseSensitivity {
    #[default]
    Insensitive,
    Sensitive,
}

/// One parsed query.
///
/// The terms are folded once here rather than at each fact they are compared with, so one
/// query asked of a whole Package folds each term exactly once.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchQuery {
    terms: Vec<String>,
    case: CaseSensitivity,
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
        }
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

/// The Orders of `package` that match `query`, in the Package's own order.
///
/// Each pair is an Order's position among [`CurrentPackageCandidate::healthy_orders`] and
/// the Order itself, so the answer is a subsequence of that list and a caller resolves an
/// Order's Vendor with [`CurrentPackageCandidate::vendor_for_order`].
///
/// A Damaged Order is absent, and not by being filtered: its record could not be read or
/// did not validate, so it has no facts to match on and never enters the list this walks.
pub fn matching_orders<'a>(
    package: &'a CurrentPackageCandidate,
    query: &SearchQuery,
) -> Vec<(usize, &'a HealthyOrder)> {
    package
        .healthy_orders()
        .iter()
        .enumerate()
        .filter(|(position, order)| {
            let vendor = package.vendor_for_order(*position);
            query.retains(&order_facts(order, vendor, query))
        })
        .collect()
}

/// The Vendors of `package` that match `query`, in the Package's own order.
///
/// Every Vendor, Hidden ones included — [`CurrentPackageCandidate::vendors`] is where the
/// reason lives. Which of them a caller then shows is that caller's own control.
pub fn matching_vendors<'a>(
    package: &'a CurrentPackageCandidate,
    query: &SearchQuery,
) -> Vec<&'a Vendor> {
    package
        .vendors()
        .iter()
        .filter(|vendor| query.retains(&vendor_facts(vendor, query)))
        .collect()
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

    /// One stored number, as the shortest decimal that reads back as the same number.
    ///
    /// A quantity's only spelling, which Define lookup and search behavior is where the
    /// reason lives.
    fn add_stored_number(&mut self, value: f64) {
        self.add(&value.to_string());
    }

    /// One monetary fact, by the spelling its record stores and by the spelling a reader
    /// is shown.
    ///
    /// The second is [`two_decimal_spelling`]'s rather than a convention spelled again
    /// here, which is what makes the text a search compares the text a reader was shown.
    ///
    /// Unit Price is the only monetary fact reached from here. The Item Total and the
    /// Order Total are calculated, and a calculated total is one of the exclusions.
    fn add_monetary_number(&mut self, value: f64) {
        self.add_stored_number(value);
        self.add(&two_decimal_spelling(value));
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
    let document = order.document();
    let mut facts = query.facts();
    facts.add(document.order_id.as_str());
    facts.add(document.title.as_str());
    facts.add(document.date_ordered.as_str());
    facts.add(document.status.as_str());
    // The Vendor Reference the record stores: a canonical Order fact like any other, and
    // named in none of the decision's exclusions, so one Vendor ID reaches the Vendor and
    // the Orders that name it alike.
    facts.add_optional(document.vendor_id.as_ref().map(VendorId::as_str));
    facts.add_optional(document.orderer.as_deref());
    facts.add_optional(document.ordering_institution.as_deref());
    facts.add_optional(document.currency.as_ref().map(Currency::as_str));
    facts.add_optional(document.project.as_deref());
    facts.add_optional(document.order_group.as_deref());
    for item in &document.items {
        facts.add(item.description.as_str());
        facts.add_optional(item.catalog_number.as_deref());
        facts.add_stored_number(item.quantity.get());
        facts.add_monetary_number(item.unit_price.get());
    }
    // The README the load established for this Order. `None` is a document that exists and
    // could not be read, so there is no text to match on and none is invented — the load's
    // diagnostics are what say why.
    facts.add_markdown(order.readme_text());
    if let Some(vendor) = vendor {
        add_shared_vendor_facts(&mut facts, vendor);
    }
    facts
}

/// One Vendor's searchable facts: its identity and its shared facts.
fn vendor_facts(vendor: &Vendor, query: &SearchQuery) -> Facts {
    let mut facts = query.facts();
    facts.add(vendor.vendor_id.as_str());
    add_shared_vendor_facts(&mut facts, vendor);
    facts
}

/// Every current shared fact of one Vendor, Contacts included.
///
/// Visibility is not among them. **A choice, not a rule:** a Hidden Vendor is in a state
/// rather than carrying a fact, and this core spells no text for that state, so there is
/// nothing here for a term to be a substring of.
fn add_shared_vendor_facts(facts: &mut Facts, vendor: &Vendor) {
    facts.add(vendor.name.as_str());
    facts.add_optional(vendor.phone.as_deref());
    facts.add_optional(vendor.email.as_deref());
    facts.add_optional(vendor.website.as_deref());
    facts.add_optional(vendor.address.as_deref());
    facts.add_optional(vendor.purchasing_notes.as_deref());
    for contact in &vendor.contacts {
        facts.add(contact.name.as_str());
        facts.add_optional(contact.role.as_deref());
        facts.add_optional(contact.phone.as_deref());
        facts.add_optional(contact.email.as_deref());
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
