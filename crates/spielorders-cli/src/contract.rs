//! The registry of response contract identities.
//!
//! Every command owns an independently evolving response contract. `contract` names
//! the exact command shape and `contract_version` is that response's own consecutive
//! positive integer — independent of Package `metadata_version`, of any managed file's
//! `schema_version`, of the release version, and of every other response contract.
//!
//! This module is the only place a `contract` string exists. A command emits an
//! envelope by naming a variant here, so two commands cannot collide on an identity
//! and no command can spell one inline. That is the whole point of a registry: the
//! identities have to be comparable in one place to be kept distinct.
//!
//! **Nothing below is in force yet.** No `contract_version` advances before the release
//! the implementation roadmap names, under "Versioning before release". Until then a
//! change to a member, a payload, an enum value or a diagnostic's meaning edits version 1
//! and re-records that contract's cells. From that release on, the rules below take
//! effect.
//!
//! **Advancing a version.** An incompatible change to a field, its representation, its
//! requiredness, an enum value, a diagnostic's meaning, or the payload advances only the
//! affected contract. Formatting and human wording advance nothing.
//!
//! **Incompatible is the operative word.** A new enum or diagnostic value reporting a
//! condition previously unreportable is additive and advances nothing, because a consumer
//! branching on the old values already tolerates one it does not know. Changing, removing,
//! or re-meaning an existing value advances the affected contract, as does adding or
//! removing a member or changing its type or requiredness.
//!
//! Decision sources: Define the Rust CLI capability contract.

pub use crate::commands::search::supporting_material_document;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseContract {
    /// Every usage error, whichever command line produced it.
    ///
    /// **A usage error is not a command's response, and does not carry a command's
    /// identity.** Exit `2` exists precisely because nothing about the subject was
    /// attempted: parsing failed, so no command ran and no command has a response to
    /// give. `contract` "identifies the exact command shape", and no command shape ran
    /// — while the rule that a failure "retains *the command's* contract" presupposes a
    /// command that did. This holds however far the parse got: naming a command that
    /// would have been valid is not running it, and `spielorders --json` names none at
    /// all.
    ///
    /// One identity for every usage error rather than one per command, for two reasons.
    /// A consumer handling exit `2` then branches once instead of having to know every
    /// command's identity to read a malformed-command-line response. And the alternative
    /// is one fixture per command identical but for this string, which is the same
    /// duplication one level up, and as many versions to advance in step whenever the
    /// usage envelope changes at all.
    Usage,
    /// `spielorders init`
    ///
    /// **The first mutation contract**, and the one every later mutation takes its report
    /// shape from. `--dry-run` changes neither the identity nor the shape; see
    /// [`crate::commands::init`].
    Init,
    /// `spielorders package readme`
    PackageReadme,
    /// `spielorders package show`
    PackageShow,
    /// `spielorders package list orders`
    PackageListOrders,
    /// `spielorders package list vendors`
    PackageListVendors,
    /// `spielorders package values ordering_institution`
    ///
    /// **The four value kinds are four contracts, not one contract with a `kind`
    /// member.** `contract` names the exact command shape, and
    /// `package values ordering_institution` and `package values order_group` are two
    /// command shapes — so each versions independently, and a change to one of them
    /// advances nothing about the other three. The `<kind>` segment is the Order record's
    /// own field name, which is also the token the command line carries; see
    /// [`crate::cli::ValueKind`], which is also where these four take their order from.
    PackageValuesOrderingInstitution,
    /// `spielorders package values orderer`
    PackageValuesOrderer,
    /// `spielorders package values project`
    PackageValuesProject,
    /// `spielorders package values order_group`
    PackageValuesOrderGroup,
    /// `spielorders order show`, and `spielorders order item list`
    ///
    /// **Two command lines, one contract.** See the read-payload bullets of Define the
    /// Rust CLI capability contract, and the owner ruling recorded beneath them.
    OrderShow,
    /// `spielorders order edit`
    ///
    /// **The refreshed Order is [`Self::OrderShow`]'s projection, under this identity**,
    /// exactly as every other Order mutation carries it under its own — see
    /// [`Self::AddOrder`]. What an edit adds beside it is the reuse-versus-introduction
    /// report for the four Reusable Free-Entry Value categories, and where the committed
    /// Date Ordered came from: an edit is the one command that can apply one from the
    /// clock without being asked for a date.
    OrderEdit,
    /// `spielorders order item add`
    ///
    /// **The four Item mutations are four contracts**, for the reason the four Contact
    /// mutations are: `contract` names the exact command shape, and adding an Item,
    /// editing one, moving one, and removing one are four shapes.
    ///
    /// **An Item has no contract of its own**: it is embedded in the Order that owns it, so
    /// what these commands answer with is that Order — [`Self::OrderShow`]'s projection
    /// under these identities — beside a report of what they did to its collection.
    OrderItemAdd,
    /// `spielorders order item edit`
    OrderItemEdit,
    /// `spielorders order item move`
    OrderItemMove,
    /// `spielorders order item remove`
    ///
    /// **An Item is never disposed of anywhere**, so this is a removal in the ordinary
    /// sense and not `order remove`'s: there is no Removed collection, no Trash, and no
    /// disposition to report. What it answers is the report of what it removed and the
    /// Order that no longer holds it.
    OrderItemRemove,
    /// `spielorders order validate`
    ///
    /// **A sibling of [`Self::Validate`], not a version of it.** The two answer the same
    /// question about two scopes, and the identity is what says which — so a change to
    /// what an Order's scope answers must not advance the Package-wide contract.
    OrderValidate,
    /// `spielorders order readme`
    ///
    /// **A sibling of `package readme`, not a version of it.** The two answer the same
    /// three facts about two different documents, and the identity is what says which —
    /// which is exactly why they cannot share one: a change to the Order README's answer
    /// must not advance the Package README's contract, and one identity for both would
    /// make every such change advance both.
    OrderReadme,
    /// `spielorders order align`
    ///
    /// **The refreshed Order is [`Self::OrderShow`]'s projection, under this identity**,
    /// exactly as the two creating commands carry it under theirs — see
    /// [`Self::AddOrder`]. What alignment adds beside it is where the Directory was and
    /// where it now is, which is the one pair of old and new paths a mutation report
    /// carries "where relevant" and which no other Order mutation has.
    OrderAlign,
    /// `spielorders order remove`, with or without `--delete`
    ///
    /// **One command, one contract, and the disposition is data.** `--delete` chooses
    /// where the directory goes; it does not make a second command shape, any more than
    /// `--all` makes a second `package show` or `--dry-run` a second anything. The
    /// decided command tree writes it as one command's option, and the response reports
    /// which disposition ran.
    ///
    /// **Its payload is not [`Self::OrderShow`]'s.** A removal answers with the removed
    /// entity summary and its disposition, which the CLI contract names as its own thing
    /// — and it is: the Order is no longer one of this Package's Orders, so a projection
    /// that reported its current paths, README, and local diagnostics would be reporting
    /// them about a directory this Package no longer holds.
    OrderRemove,
    /// `spielorders vendor show`, and `spielorders vendor contact list`
    ///
    /// **Two command lines, one contract.** See the read-payload bullets of Define the
    /// Rust CLI capability contract, and the owner ruling recorded beneath them.
    VendorShow,
    /// `spielorders add-order`
    ///
    /// **A sibling of [`Self::AdoptOrder`], not a version of it.** Both give an Order
    /// Directory its first record and they are two command shapes: one creates the
    /// directory, the other brings an existing one under management without touching
    /// anything inside it. A change to what adoption answers must not advance the contract
    /// of the command that creates.
    ///
    /// **Its refreshed Order is [`Self::OrderShow`]'s projection, under this identity**,
    /// exactly as a Vendor mutation's refreshed Vendor is — see [`Self::AddVendor`]. The
    /// one member that projection carries for these two alone is a `null` `order_id`: an
    /// Order ID is minted at the commit, so a run asked to plan only has none to report and
    /// reporting the planning placeholder would report an identity that will never exist.
    AddOrder,
    /// `spielorders adopt-order`
    AdoptOrder,
    /// `spielorders add-vendor`
    ///
    /// **The four Vendor mutations are four contracts.** `contract` names the exact
    /// command shape, and creating a Vendor, editing one, hiding one, and unhiding one are
    /// four shapes — so each versions independently and a change to one advances nothing
    /// about the other three. It is the same reading that makes the four
    /// `package values <kind>` command lines four contracts.
    ///
    /// **A mutation's refreshed Vendor is [`Self::VendorShow`]'s projection, under this
    /// identity.** The owner ruling that one projection serves a show and its sub-list is
    /// about what a *response renders*, not about which command answered: the projection
    /// is built once, and these four carry it beside a report of what they did. Sharing
    /// `vendor.show`'s identity instead would make a change to the mutation report advance
    /// the read's contract.
    AddVendor,
    /// `spielorders vendor edit`
    VendorEdit,
    /// `spielorders vendor hide`
    VendorHide,
    /// `spielorders vendor unhide`
    ///
    /// **A sibling of [`Self::VendorHide`], not a version of it.** The two are one
    /// reversible pair and still two command shapes, so a change to what unhiding answers
    /// must not advance the contract of the command that hides.
    VendorUnhide,
    /// `spielorders vendor contact add`
    ///
    /// **The four Contact mutations are four contracts**, for the reason the four Vendor
    /// mutations are: `contract` names the exact command shape, and adding a Contact,
    /// editing one, moving one, and removing one are four shapes.
    ///
    /// **Their refreshed Vendor is [`Self::VendorShow`]'s projection, under these
    /// identities**, exactly as a Vendor mutation's is — see [`Self::AddVendor`]. A
    /// Contact has no contract of its own: it is embedded in the Vendor that owns it, so
    /// what these commands answer with is that Vendor, beside a report of what they did to
    /// its collection.
    VendorContactAdd,
    /// `spielorders vendor contact edit`
    VendorContactEdit,
    /// `spielorders vendor contact move`
    VendorContactMove,
    /// `spielorders validate`
    ///
    /// **Validity is `ok`, and the findings are `diagnostics`.** A result is invalid
    /// exactly when some finding carries error severity, which is the predicate the core
    /// computes and the one the exit status is decided on — and `ok` is that exit status.
    /// So `data` carries neither: it carries what this run examined, which is the one
    /// thing nothing else in the document says.
    Validate,
    /// `spielorders repair`, diagnosing and applying alike
    ///
    /// **One command, one contract, and the two moods are data.** `case=… action=…`
    /// chooses whether this run applies anything; it does not make a second command shape,
    /// any more than `--dry-run` makes a second anything. The payload carries what is
    /// actionable now either way, and `report` is `null` for the run that applied nothing.
    ///
    /// **A sibling of [`Self::OrderRepair`], not a version of it**, for the reason the two
    /// validating commands are siblings: they answer the same question about two scopes,
    /// and the identity is what says which.
    Repair,
    /// `spielorders order repair`
    OrderRepair,
    /// `spielorders upgrade`, with or without `--dry-run`
    ///
    /// **One command, one contract, and the plan is the answer either way.** `--dry-run`
    /// chooses whether the plan is carried out; it does not make a second command shape,
    /// any more than it does for any other mutation. What the response reports is where
    /// this Package's metadata is, where this build would take it, every artifact in the
    /// deterministic order the run visits them, and what the run did — see
    /// [`crate::commands::upgrade`].
    Upgrade,
    /// `spielorders index status`
    ///
    /// **The one contract whose subject is the derived index.** Every other response
    /// reports the index beside something else — the Package it read, or the record a
    /// mutation left — and reports it in the same shape; here it is the answer.
    IndexStatus,
    /// `spielorders index rebuild`
    ///
    /// **A sibling of [`Self::IndexStatus`], not a version of it.** One reports the index
    /// and the other replaces it, and they are two command shapes.
    IndexRebuild,
    /// `spielorders vendor contact remove`
    ///
    /// **A Contact is never disposed of anywhere**, so this is a removal in the ordinary
    /// sense and not `order remove`'s: there is no Removed collection, no Trash, and no
    /// disposition to report. What it answers is the report of what it removed and the
    /// Vendor that no longer holds it.
    VendorContactRemove,
    /// `spielorders search orders`
    ///
    /// **A sibling of [`Self::SearchVendors`], not a version of it**, for the reason the
    /// two validating commands are siblings: they answer the same kind of question about
    /// two different subjects, and the identity is what says which. A change to what a
    /// matched Order carries must not advance the Vendor search's contract.
    ///
    /// **Its Orders are [`Self::PackageListOrders`]'s summaries, under this identity.** A
    /// matched Order and a listed Order are one thing, so the projection is shared; what
    /// this contract carries beside them, and what it leaves out, is
    /// [`crate::commands::search`]'s to say.
    SearchOrders,
    /// `spielorders search vendors`
    ///
    /// **Its Vendors are [`Self::PackageListVendors`]'s summaries, under this identity**,
    /// and it carries that command's `hidden_vendors_included` beside them.
    SearchVendors,
    /// `spielorders search supporting-material`
    ///
    /// **Its own contract beside the two entity searches**, which are one command shape
    /// each like every other: the three answer different questions about different
    /// subjects, and this one's payload is a platform provider's answer rather than a
    /// projection of the Package's own records.
    ///
    /// **A provider that could not look is a failure, not an empty answer.** The question
    /// is which Supporting Material matches, and a state that prevented anything from
    /// looking is the reason there is no answer rather than an answer — so it reaches
    /// exit `1`, under a code naming the state, exactly as a read that could not read
    /// does. An index that looked and found nothing is the other thing entirely: it
    /// answers, at exit `0`, with no groups.
    SearchSupportingMaterial,
}

impl ResponseContract {
    /// Every response contract this executable can emit, each exactly once.
    ///
    /// This slice is the registry. The exhaustive matches in [`Self::name`] and
    /// [`Self::version`] make a new variant a compile error until it is named and
    /// versioned; `tests/response_contract.rs` reads this slice and is what makes a
    /// missing entry, a duplicate identity, a malformed name, or a contract with no
    /// golden fixtures a test failure.
    pub const REGISTERED: &'static [Self] = &[
        Self::Usage,
        Self::Init,
        Self::PackageReadme,
        Self::PackageShow,
        Self::PackageListOrders,
        Self::PackageListVendors,
        Self::PackageValuesOrderingInstitution,
        Self::PackageValuesOrderer,
        Self::PackageValuesProject,
        Self::PackageValuesOrderGroup,
        Self::OrderShow,
        Self::OrderEdit,
        Self::OrderItemAdd,
        Self::OrderItemEdit,
        Self::OrderItemMove,
        Self::OrderItemRemove,
        Self::OrderReadme,
        Self::OrderValidate,
        Self::OrderAlign,
        Self::OrderRemove,
        Self::AddOrder,
        Self::AdoptOrder,
        Self::VendorShow,
        Self::AddVendor,
        Self::VendorEdit,
        Self::VendorHide,
        Self::VendorUnhide,
        Self::VendorContactAdd,
        Self::VendorContactEdit,
        Self::VendorContactMove,
        Self::VendorContactRemove,
        Self::Validate,
        Self::Repair,
        Self::OrderRepair,
        Self::Upgrade,
        Self::IndexStatus,
        Self::IndexRebuild,
        Self::SearchOrders,
        Self::SearchVendors,
        Self::SearchSupportingMaterial,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Usage => "spielorders.usage",
            Self::Init => "spielorders.init",
            Self::PackageReadme => "spielorders.package.readme",
            Self::PackageShow => "spielorders.package.show",
            Self::PackageListOrders => "spielorders.package.list.orders",
            Self::PackageListVendors => "spielorders.package.list.vendors",
            Self::PackageValuesOrderingInstitution => {
                "spielorders.package.values.ordering_institution"
            }
            Self::PackageValuesOrderer => "spielorders.package.values.orderer",
            Self::PackageValuesProject => "spielorders.package.values.project",
            Self::PackageValuesOrderGroup => "spielorders.package.values.order_group",
            Self::OrderShow => "spielorders.order.show",
            Self::OrderEdit => "spielorders.order.edit",
            Self::OrderItemAdd => "spielorders.order.item.add",
            Self::OrderItemEdit => "spielorders.order.item.edit",
            Self::OrderItemMove => "spielorders.order.item.move",
            Self::OrderItemRemove => "spielorders.order.item.remove",
            Self::OrderReadme => "spielorders.order.readme",
            Self::OrderValidate => "spielorders.order.validate",
            Self::OrderAlign => "spielorders.order.align",
            Self::OrderRemove => "spielorders.order.remove",
            Self::AddOrder => "spielorders.add_order",
            Self::AdoptOrder => "spielorders.adopt_order",
            Self::VendorShow => "spielorders.vendor.show",
            Self::AddVendor => "spielorders.add_vendor",
            Self::VendorEdit => "spielorders.vendor.edit",
            Self::VendorHide => "spielorders.vendor.hide",
            Self::VendorUnhide => "spielorders.vendor.unhide",
            Self::VendorContactAdd => "spielorders.vendor.contact.add",
            Self::VendorContactEdit => "spielorders.vendor.contact.edit",
            Self::VendorContactMove => "spielorders.vendor.contact.move",
            Self::VendorContactRemove => "spielorders.vendor.contact.remove",
            Self::Validate => "spielorders.validate",
            Self::Repair => "spielorders.repair",
            Self::OrderRepair => "spielorders.order.repair",
            Self::Upgrade => "spielorders.upgrade",
            Self::IndexStatus => "spielorders.index.status",
            Self::IndexRebuild => "spielorders.index.rebuild",
            Self::SearchOrders => "spielorders.search.orders",
            Self::SearchVendors => "spielorders.search.vendors",
            Self::SearchSupportingMaterial => "spielorders.search.supporting_material",
        }
    }

    pub fn version(self) -> u32 {
        match self {
            Self::Usage => 1,
            Self::Init => 1,
            Self::PackageReadme => 1,
            Self::PackageShow => 1,
            Self::PackageListOrders => 1,
            Self::PackageListVendors => 1,
            Self::PackageValuesOrderingInstitution => 1,
            Self::PackageValuesOrderer => 1,
            Self::PackageValuesProject => 1,
            Self::PackageValuesOrderGroup => 1,
            Self::OrderShow => 1,
            Self::OrderEdit => 1,
            Self::OrderItemAdd => 1,
            Self::OrderItemEdit => 1,
            Self::OrderItemMove => 1,
            Self::OrderItemRemove => 1,
            Self::OrderReadme => 1,
            Self::OrderValidate => 1,
            Self::OrderAlign => 1,
            Self::OrderRemove => 1,
            Self::AddOrder => 1,
            Self::AdoptOrder => 1,
            Self::VendorShow => 1,
            Self::AddVendor => 1,
            Self::VendorEdit => 1,
            Self::VendorHide => 1,
            Self::VendorUnhide => 1,
            Self::VendorContactAdd => 1,
            Self::VendorContactEdit => 1,
            Self::VendorContactMove => 1,
            Self::VendorContactRemove => 1,
            Self::Validate => 1,
            Self::Repair => 1,
            Self::OrderRepair => 1,
            Self::Upgrade => 1,
            Self::IndexStatus => 1,
            Self::IndexRebuild => 1,
            Self::SearchOrders => 1,
            Self::SearchVendors => 1,
            Self::SearchSupportingMaterial => 1,
        }
    }

    /// Where this contract's golden fixtures live, relative to `contracts/cli/`.
    ///
    /// Derived from the identity rather than chosen per command: the fixture home is
    /// the contract's own name, so a contract cannot be filed under a directory that
    /// says something else, and the version segment is where a bump would land if this
    /// software had a user to break. See `docs/development/traceability.md`.
    // `expect` rather than `allow`, so removing the assertion below removes this too
    // instead of leaving a standing permission behind. The `cfg_attr` is what keeps the
    // expectation from going unfulfilled in the unit-test build, where `lib.rs` lifts
    // the lint.
    #[cfg_attr(
        not(test),
        expect(
            clippy::expect_used,
            reason = "the prefix is this crate's own contract identity, which nothing a \
                      command line carries can influence"
        )
    )]
    pub fn fixture_directory(self) -> String {
        let path = self
            .name()
            .strip_prefix("spielorders.")
            .expect("every contract identity is namespaced to this application")
            .replace('.', "/");
        format!("{path}/v{}", self.version())
    }
}
