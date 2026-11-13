//! The command line: Git-style first-level families, their commands, and the options
//! that govern targeting, output, and execution.
//!
//! **The command path comes first and its modifiers follow it.** Each option is declared
//! on the command it modifies, so a whole command line arrives at one parse level and the
//! parser decides the whole grammar. Nothing precedes the command path except `--help`
//! and `--version`, which are reference requests rather than modifiers.
//!
//! That is what lets every exclusion be declared on the group that owns it. An option
//! declared once for the executable stands in any gap between the command path's
//! segments, and a conflict is resolved against one parse level's own matches — so a pair
//! written on either side of a segment is never seen together, a repetition is one
//! occurrence at each level, and the rule has to be decided after parsing over the whole
//! command line. One parse level is what makes each of those the parser's own refusal.
//!
//! **Options never carry domain facts.** Targeting, output mode, and execution are
//! what an option is for; domain facts arrive as canonical `field=value` assignments
//! at the commands that take them.
//!
//! **The canonical assignment language is parsed here and adjudicated in the core.**
//! Splitting on the first `=` is a fact about the command line, so a quoted value may
//! contain as many more as it likes. Which fields a record has, which of them may be
//! assigned, and what an empty value means are Package rules — so an unrecognized field
//! is not a usage error here: the core refuses it, names it, and reports every other
//! refused field of the same set beside it.
//!
//! **A family appears here when it has a command.** The executable is Git-style —
//! families with subcommands, and a few first-level commands that take none, which the
//! decided command tree spells out — and this is where that shape lives, but an empty
//! family registered ahead of its commands would put a name in `--help` that answers
//! nothing. Each later command ticket adds its family here as it delivers the command.
//!
//! **Generated `--help` and `--version` are the whole reference.** There is no `help`
//! subcommand, no pager, and no full-screen mode.
//!
//! Decision sources: Define the Rust CLI capability contract.

use crate::output::Mode;
use clap::{
    Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum,
    error::{ContextKind, ContextValue, ErrorKind},
};
use spielorders::{
    SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD, SpielordersConstant,
    spielorders_constant_name,
};
use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "spielorders",
    version,
    about = "Manage a SpielOrder Package from the command line",
    long_about = None,
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub family: Family,
}

impl Cli {
    /// The output mode the named command carries.
    ///
    /// Asked of the command rather than of the executable, because that is where the
    /// option is now declared. The executable builds one [`crate::output::Output`] before
    /// it dispatches, so this is the one question it puts to the parsed command line
    /// ahead of knowing which command it is.
    pub(crate) fn mode(&self) -> Mode {
        self.family.mode()
    }
}

// Which group answers for each command, in one place rather than beside each enum,
// because it is one walk down the command path to the command that declares the option.
//
// **Exhaustive rather than read back out of the parse.** A command added to the tree does
// not compile until it says how it is reported in, which is the same guarantee the
// dispatch in `crate::command` gets and for the same reason: an accessor that searched the
// matches for whichever level happened to carry the flags would answer for a command that
// declares no output mode at all.

impl Family {
    fn mode(&self) -> Mode {
        match self {
            Self::Init { output, .. }
            | Self::AddVendor { output, .. }
            | Self::AddOrder { output, .. }
            | Self::AdoptOrder { output, .. }
            | Self::Validate { output, .. }
            | Self::Repair { output, .. } => output.mode(),
            Self::Package { command } => command.mode(),
            Self::Index { command } => command.mode(),
            Self::Order { command } => command.mode(),
            Self::Vendor { command } => command.mode(),
            Self::Search { command } => command.mode(),
            Self::Agent { command } => command.mode(),
        }
    }
}

impl SearchCommand {
    fn mode(&self) -> Mode {
        match self {
            Self::Orders { output, .. }
            | Self::Vendors { output, .. }
            | Self::SupportingMaterial { output, .. } => output.mode(),
        }
    }
}

impl AgentCommand {
    fn mode(&self) -> Mode {
        match self {
            Self::Provision { output, .. } | Self::Defaults { output } => output.mode(),
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum AgentCommand {
    /// Write the current agent guidance into any directory, replacing earlier copies of its
    /// files and touching no other file.
    Provision {
        #[command(flatten)]
        target: DirectoryTarget,
        #[command(flatten)]
        execution: Execution,
        #[command(flatten)]
        output: OutputMode,
    },
    /// Locate installed default guidance without opening a Package.
    Defaults {
        #[command(flatten)]
        output: OutputMode,
    },
}

impl PackageCommand {
    fn mode(&self) -> Mode {
        match self {
            Self::Readme { output, .. }
            | Self::Show { output, .. }
            | Self::Values { output, .. } => output.mode(),
            Self::List { command } => command.mode(),
        }
    }
}

impl ListCommand {
    fn mode(&self) -> Mode {
        match self {
            Self::Orders { output, .. } | Self::Vendors { output, .. } => output.mode(),
        }
    }
}

impl IndexCommand {
    fn mode(&self) -> Mode {
        match self {
            Self::Status { output, .. } | Self::Rebuild { output, .. } => output.mode(),
        }
    }
}

impl OrderCommand {
    fn mode(&self) -> Mode {
        match self {
            Self::Show { output, .. }
            | Self::Edit { output, .. }
            | Self::Readme { output, .. }
            | Self::Validate { output, .. }
            | Self::Repair { output, .. }
            | Self::Align { output, .. }
            | Self::Remove { output, .. } => output.mode(),
            Self::Item { command } => command.mode(),
        }
    }
}

impl ItemCommand {
    fn mode(&self) -> Mode {
        match self {
            Self::List { output, .. }
            | Self::Add { output, .. }
            | Self::Edit { output, .. }
            | Self::Move { output, .. }
            | Self::Remove { output, .. } => output.mode(),
        }
    }
}

impl VendorCommand {
    fn mode(&self) -> Mode {
        match self {
            Self::Show { output, .. }
            | Self::Edit { output, .. }
            | Self::Hide { output, .. }
            | Self::Unhide { output, .. }
            | Self::Delete { output, .. } => output.mode(),
            Self::Contact { command } => command.mode(),
        }
    }
}

impl ContactCommand {
    fn mode(&self) -> Mode {
        match self {
            Self::List { output, .. }
            | Self::Add { output, .. }
            | Self::Edit { output, .. }
            | Self::Move { output, .. }
            | Self::Remove { output, .. } => output.mode(),
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Family {
    /// Provision agent guidance into a directory or locate the installed defaults.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    // A first-level command with no subcommand of its own, because there is one thing to
    // do to a directory that is not yet a Package. Its two optional configuration keys
    // arrive as assignments rather than as options: an option governs targeting, output,
    // or execution, and where the Orders live is a fact about the Package.
    /// Make an ordinary directory a SpielOrder Package.
    ///
    /// The Package is created at the newest metadata version this build understands. An
    /// existing `README.md` in the directory is left exactly as it is; where there is
    /// none, an empty conventional Package README is created.
    Init {
        /// Package configuration, as `orders_path=…` and `removed_path=…`.
        ///
        /// Either may be omitted and takes its conventional default. Each must be a
        /// nonempty `/`-separated path inside the Package, and `removed_path` must name
        /// an ordinary direct child of it.
        #[arg(value_name = "ASSIGNMENT", value_parser = Assignment::parse)]
        assignments: Vec<Assignment>,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    // A first-level command, like `init` and unlike the reads, because a Vendor that does
    // not exist yet cannot be selected: there is no `NAME` for a `vendor` subcommand to
    // resolve. The Package it is created in is the ordinary `--root`.
    /// Add a Vendor to a SpielOrder Package.
    ///
    /// `add-vendor "Tarnwick"` is the Name shorthand for `add-vendor name=Tarnwick`.
    /// The two forms cannot be combined, and there is no create or merge alias.
    AddVendor {
        /// The Vendor Name on its own, or `field=value` assignments.
        ///
        /// The assignable facts are the Vendor's own shared ones. An empty value on a
        /// creation is simply an absent fact.
        #[arg(value_name = "NAME | ASSIGNMENT", value_parser = SubjectToken::parse)]
        subject: Vec<SubjectToken>,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    // First-level and Package-rooted for `add-vendor`'s reason: the Order it creates does
    // not exist yet, so there is no Order Directory for an `order` subcommand's `--root` to
    // name. Where the directory goes is the Package's own configured Orders Directory.
    /// Add an Order to a SpielOrder Package.
    ///
    /// `add-order "Laser components"` is the Title shorthand for
    /// `add-order title="Laser components"`. The two forms cannot be combined, and there
    /// is no create alias.
    AddOrder {
        /// The Order Title on its own, or `field=value` assignments.
        ///
        /// Every assignable Order fact may be given, `status` among them. The rest take
        /// their established defaults: today's local Date Ordered, `in_preparation`, no
        /// Vendor Reference, and no Order Items.
        #[arg(value_name = "TITLE | ASSIGNMENT", value_parser = SubjectToken::parse)]
        subject: Vec<SubjectToken>,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    // Package-rooted, like `add-order`: `--root` names the SpielOrders Root and `DIRECTORY`
    // names one existing child of its configured Orders Directory. It is not an `order`
    // command, because the directory it names is not an Order yet — there is no record
    // under it for the locator route to read.
    /// Bring an existing directory under management as an Order.
    ///
    /// The candidate is adopted where it stands. Its Supporting Material and its own
    /// `README.md` are left byte for byte as they are, nothing in it is renamed or
    /// removed, and no part of its name is read as an Order fact — a directory component
    /// is evidence for a human, never a field grammar.
    AdoptOrder {
        /// The candidate directory: one existing child of the Orders Directory.
        #[arg(value_name = "DIRECTORY")]
        directory: PathBuf,

        /// The Order's facts as `field=value`, and its Order Items each after an `item`.
        ///
        /// `title`, `date_ordered`, and `status` are required, because nothing here reads
        /// them off the directory. The remaining Order facts are optional. An adopted
        /// Order may carry Items: `item` opens one, and the assignments after it are that
        /// Item's own until the next `item`.
        #[arg(value_name = "ASSIGNMENT | item", value_parser = AdoptionToken::parse)]
        assignments: Vec<AdoptionToken>,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Read a SpielOrder Package.
    Package {
        #[command(subcommand)]
        command: PackageCommand,
    },
    // A first-level command with no subcommand, because there is one thing to ask of a
    // whole Package: whether it is valid, and what is wrong with it where it is not. It
    // carries no `Execution`, because validation reports and has no mutation to withhold.
    /// Validate a complete SpielOrder Package.
    ///
    /// Every Order Directory and every cross-record constraint, with every diagnostic
    /// returned at once rather than stopping at the first. Nothing is changed: there is no
    /// `--fix` and no option that acts on a finding.
    Validate {
        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    // A first-level command with no subcommand, because diagnosing this Package's own
    // actionable cases and applying one of them are one command in two moods rather than
    // two commands. `case=` and `action=` are what tell them apart, and they arrive as
    // assignments for the reason every domain fact does.
    /// Diagnose this Package's own actionable repair cases, or apply one of them.
    ///
    /// With no assignments it diagnoses and changes nothing. `case=… action=…` applies
    /// exactly one action the diagnosis offered, with `field=value` resolutions where the
    /// action needs one. There is no `--fix`, no repair-all, and nothing here is inferred.
    ///
    /// A case inside an Order Directory is `spielorders order repair`'s; neither reaches
    /// into the other's scope.
    Repair {
        /// `case=…` and `action=…`, and the resolutions the action requires.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    // Package-rooted, because the derived index belongs to the Package rather than to any
    // Order: `--root` names the SpielOrders Root.
    /// Report or replace this Package's derived index.
    ///
    /// The derived index is wholly a projection of the canonical records, so it is
    /// rebuilt rather than repaired. Neither command here repairs canonical metadata or
    /// inspects Supporting Material.
    Index {
        #[command(subcommand)]
        command: IndexCommand,
    },
    // Every command here targets *one* Order. A Package's collections are `package list`'s,
    // so there is no `order list` — an Order-rooted command that listed the Package's
    // Orders would have to target one Order in order to report its siblings.
    /// Read, change, or relocate one Order of a SpielOrder Package.
    ///
    /// For these commands `--root` names the Order Directory rather than the SpielOrders
    /// Root, and the Package is reached through the Order's own technical locator. No
    /// ancestor is searched.
    Order {
        #[command(subcommand)]
        command: OrderCommand,
    },
    // Package-rooted, because Vendors have no directories: `--root` names the SpielOrders
    // Root and `NAME` names the Vendor within it. There is no `--uuid` here and no
    // selection by Vendor ID at all — Names are the selector, and machine responses return
    // the IDs.
    /// Read or change one Vendor of a SpielOrder Package.
    ///
    /// `NAME` is the Vendor Name, resolved by the established trimmed, case-insensitive,
    /// package-unique comparison. A Hidden Vendor resolves like any other.
    Vendor {
        #[command(subcommand)]
        command: VendorCommand,
    },
    // Package-rooted, because what a lookup answers about is one Package: `--root` names
    // the SpielOrders Root, and for the Supporting Material search it is also what the
    // Order Directories the query is scoped to are taken from.
    /// Look something up in a SpielOrder Package.
    Search {
        #[command(subcommand)]
        command: SearchCommand,
    },
}

// The three in the decided command surface's own order: the two deterministic entity
// searches, then the one that asks a platform service.
#[derive(Debug, Subcommand)]
pub(crate) enum SearchCommand {
    /// Report the Orders of this Package that match a query.
    ///
    /// An Order matches when every term lands on some searchable fact of it or of the
    /// Vendor it references, so editing a Vendor changes which Orders match. A matching
    /// query answers with the Orders it retained in the Package's own ordering, and a
    /// query nothing matches answers with none of them.
    Orders {
        #[command(flatten)]
        keywords: Keywords,

        #[command(flatten)]
        matching: Matching,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Report the Vendors of this Package that match a query.
    ///
    /// A Vendor matches on its own identity and shared facts, Contacts included. An
    /// Order's facts are never among them: which Orders match is the sibling command's
    /// answer.
    Vendors {
        // The same presentation option `package show` and `package list vendors` carry,
        // with the same default and for the same reason: this answer is a list of Vendors,
        // and a Vendor list offers only the active ones until `--all` asks for every one
        // with its visibility.
        /// Report Hidden Vendors too, each carrying its visibility.
        ///
        /// Without it, the Vendors this Package offers for new use are reported.
        #[arg(long)]
        all: bool,

        #[command(flatten)]
        keywords: Keywords,

        #[command(flatten)]
        matching: Matching,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    // No `--case-sensitive` and no structured filter, and that is the decision rather than
    // an omission: the platform's own index governs matching here, so a control that
    // changed how a term is compared would be this application claiming a semantics it does
    // not implement. The two entity searches are where the deterministic query lives.
    /// Find Supporting Material with the platform's own index.
    ///
    /// The query is scoped to this Package's Order Directories and its results are grouped
    /// under the Orders that hold them. SpielOrders never walks or indexes Supporting
    /// Material itself: where no provider can look, the answer says which platform state
    /// prevented one.
    #[command(name = "supporting-material")]
    SupportingMaterial {
        /// The words to look for, in the provider's own matching semantics.
        ///
        /// Written as the caller wrote them. Nothing here tokenizes or folds the text, and
        /// several arguments are one query with a space between them.
        #[arg(value_name = "QUERY", required = true)]
        query: Vec<String>,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum VendorCommand {
    /// Report this Vendor's current facts, its Contacts numbered from one, and the
    /// Orders that reference it.
    Show {
        /// The Vendor Name.
        name: String,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Change this Vendor's shared facts.
    ///
    /// The assignments are one order-independent atomic mutation over the whole Vendor.
    /// An empty value clears an optional fact; a blank Vendor Name is refused.
    Edit {
        /// The Vendor Name.
        name: String,

        /// The facts to assign, as `field=value`.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    // Hiding and unhiding govern whether a Vendor is offered for new use, never whether
    // it resolves, so neither is refused on reference grounds and either can be undone by
    // the other. Only `delete` removes a Vendor, and only one no Order references.
    /// Stop offering this Vendor for new use.
    ///
    /// Its record and its Vendor ID stay where they are, so every Order that references
    /// it keeps resolving its current facts.
    Hide {
        /// The Vendor Name.
        name: String,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Offer this Vendor for new use again.
    Unhide {
        /// The Vendor Name.
        name: String,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Delete this Vendor and its Contacts.
    ///
    /// Refused while any Order references the Vendor, listing those Orders; hide a Vendor
    /// that is still in use instead. Deletion cannot be undone by another command.
    Delete {
        /// The Vendor Name.
        name: String,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Work with this Vendor's ordered Contact collection.
    Contact {
        #[command(subcommand)]
        command: ContactCommand,
    },
}

// Contacts are embedded in the Vendor that owns them and have no identities, so every
// command here names its Vendor and then a one-based `POSITION` in that Vendor's ordered
// collection. `POSITION` is taken as written rather than parsed into a number: which
// numbers name a Contact is a fact about the collection, so all four ways of getting one
// wrong — zero, negative, not a number, past the end — are one refused mutation under one
// code rather than a usage error for two of them and a refusal for the others.
#[derive(Debug, Subcommand)]
pub(crate) enum ContactCommand {
    /// List this Vendor's Contacts, numbered from one.
    List {
        /// The Vendor Name.
        name: String,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Add a Contact to this Vendor's ordered collection.
    ///
    /// The Contact is appended unless `position=N` says where to insert it, in which case
    /// the Contacts from there on shift down. A Contact requires a Name; its role, phone,
    /// and email are optional.
    Add {
        /// The Vendor Name.
        name: String,

        /// The Contact's facts as `field=value`, and optionally `position=N`.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Change the facts of the Contact at this position.
    ///
    /// The assignments are one order-independent atomic mutation over that Contact. An
    /// empty value clears an optional fact; a blank Contact Name is refused.
    Edit {
        /// The Vendor Name.
        name: String,

        /// The Contact's one-based position in this Vendor's collection.
        #[arg(value_name = "POSITION", allow_negative_numbers = true)]
        position: String,

        /// The facts to assign, as `field=value`.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Move the Contact at this position to another position.
    ///
    /// Both positions are evaluated against the collection as it stands before the move.
    /// Moving a Contact to the position it already occupies changes nothing and is not an
    /// error.
    Move {
        /// The Vendor Name.
        name: String,

        /// The Contact's one-based position in this Vendor's collection.
        #[arg(value_name = "POSITION", allow_negative_numbers = true)]
        position: String,

        /// Where the Contact goes, as `position=N`.
        #[arg(value_name = "position=POSITION", value_parser = Destination::parse)]
        destination: Destination,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Remove the Contact at this position.
    ///
    /// A Vendor's last Contact may be removed: its Contacts are then a collection that is
    /// present and empty, which is what a Vendor created without one has.
    Remove {
        /// The Vendor Name.
        name: String,

        /// The Contact's one-based position in this Vendor's collection.
        #[arg(value_name = "POSITION", allow_negative_numbers = true)]
        position: String,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
}

/// Where a `move` puts the member it took.
///
/// **The key is checked here and the value is not**, which is the same split
/// [`Assignment`] makes and for the same reason: `position` is not a fact of the member the
/// core adjudicates but targeting this adapter must read before it can call a core
/// capability, so a token naming something else has named nothing this command has. Which
/// numbers name a member is the collection's own business, so the value travels as written.
///
/// One type for both ordered collections, because a destination is one notion: a Contact's
/// and an Item's are the same token with the same key, and two types would be two places
/// for the rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Destination {
    pub position: String,
}

impl Destination {
    fn parse(argument: &str) -> Result<Self, String> {
        let (key, value) = argument
            .split_once('=')
            .ok_or_else(|| format!("a destination is `{POSITION_KEY}=POSITION`"))?;
        if key != POSITION_KEY {
            return Err(format!("a `move` takes `{POSITION_KEY}=POSITION`"));
        }
        Ok(Self {
            position: value.to_owned(),
        })
    }
}

/// The insertion point an `add` command line may supply, and the member's own facts.
///
/// **`position` is not a fact of the member**, so it is taken out of the assignment set
/// rather than handed to the core: a Contact has a Name, a role, a phone, and an email, an
/// Item has a Description, a Quantity, a Unit Price, a Catalog Number, and Notes, and where either
/// goes in its collection is not one of them. Every remaining assignment is the Package's
/// own to adjudicate — an unknown field, a repeated one, and a blank required one are all
/// the core's refusals, reported together.
///
/// One function for both ordered collections, so the insertion point is separated one way
/// rather than once per collection.
///
/// Total, because [`parse`] has already refused every command line that assigns
/// `position` more than once.
pub(crate) fn insertion_assignments(
    assignments: &[FieldAssignment],
) -> (Option<&str>, Vec<FieldAssignment>) {
    let position = assignments
        .iter()
        .find(|assignment| assignment.field == POSITION_KEY)
        .map(|assignment| assignment.value.as_str());
    let facts = assignments
        .iter()
        .filter(|assignment| assignment.field != POSITION_KEY)
        .cloned()
        .collect();
    (position, facts)
}

/// The key that names a position in an ordered collection.
///
/// One spelling for the `add` insertion point and the `move` destination, because they are
/// one notion — and the same token the decided command tree writes.
const POSITION_KEY: &str = "position";

/// `init`'s configured Orders Directory.
pub(crate) const ORDERS_PATH: &str = "orders_path";

/// `init`'s configured Removed Orders Directory.
pub(crate) const REMOVED_PATH: &str = "removed_path";

/// Every repair action the library names, in its own spelling.
fn repair_actions() -> impl Iterator<Item = &'static str> {
    (SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD.0..)
        .map(|value| spielorders_constant_name(SpielordersConstant(value)))
        .map_while(|name| name.to_str().filter(|name| !name.is_empty()))
}

fn is_repair_action(text: &str) -> bool {
    repair_actions().any(|action| action == text)
}

/// The key that names one ephemeral diagnosed case.
const CASE_KEY: &str = "case";

/// The key that names one action that case offered.
const ACTION_KEY: &str = "action";

/// What a `repair` command line asked for.
///
/// **Two moods of one command**, told apart by whether it named a case and an action.
/// [`parse`] has already refused every command line that named one without the other, that
/// named either twice, that named an action this core has no notion of, or that supplied a
/// resolution with no action to resolve — so this is total, and nothing downstream has to
/// consider a half-named application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RepairRequest {
    /// Diagnose the scope and change nothing.
    Diagnose,
    /// Apply exactly one action of exactly one diagnosed case.
    Apply {
        /// The ephemeral handle the diagnosis reported. Not an identity, never persisted.
        case: String,
        action: String,
        /// Every assignment that is not `case` or `action`. Which of them the action needs
        /// is the core's answer: the diagnosed case's `required_values`.
        resolutions: Vec<FieldAssignment>,
    },
}

/// The request one `repair` command line carries.
///
/// **The action's spelling is the library's**, read through its name table so this
/// adapter never carries a second table of those words.
pub(crate) fn repair_request(assignments: &[FieldAssignment]) -> RepairRequest {
    let named = |key: &str| {
        assignments
            .iter()
            .find(|assignment| assignment.field == key)
            .map(|assignment| assignment.value.as_str())
    };
    let (Some(case), Some(action)) = (named(CASE_KEY), named(ACTION_KEY)) else {
        return RepairRequest::Diagnose;
    };
    if !is_repair_action(action) {
        return RepairRequest::Diagnose;
    }
    RepairRequest::Apply {
        case: case.to_owned(),
        action: action.to_owned(),
        resolutions: assignments
            .iter()
            .filter(|assignment| assignment.field != CASE_KEY && assignment.field != ACTION_KEY)
            .cloned()
            .collect(),
    }
}

/// Why this `repair` command line names no application, where it half-names one.
///
/// **These are usage errors rather than refused mutations**, for `init`'s reason: `case`
/// and `action` are what tell the diagnosing mood from the applying one, so a command line
/// that names one without the other, or names an action this build has no notion of, has
/// not said which of the two it is asking for — and nothing about the Package was
/// attempted. A resolution supplied with no action to resolve is the same: it names a
/// value for a repair the command line never asked for.
///
/// What is *not* refused here is an action a diagnosed case did not offer, or a resolution
/// the action does not need. Both are perfectly formed applications the core refuses on
/// the evidence, at exit `1`, naming the case.
fn incomplete_repair_request(assignments: &[FieldAssignment]) -> Option<String> {
    for key in [CASE_KEY, ACTION_KEY] {
        if assignments
            .iter()
            .filter(|assignment| assignment.field == key)
            .count()
            > 1
        {
            return Some(format!("`{key}` is assigned twice"));
        }
    }
    let named = |key: &str| assignments.iter().any(|assignment| assignment.field == key);
    match (named(CASE_KEY), named(ACTION_KEY)) {
        (true, false) => {
            return Some(format!(
                "`{CASE_KEY}` names a diagnosed case and `{ACTION_KEY}` names what to do \
                 about it; a `repair` applies one only when it has both"
            ));
        }
        (false, true) => {
            return Some(format!(
                "`{ACTION_KEY}` names what to do and `{CASE_KEY}` names the diagnosed case \
                 to do it to; a `repair` applies one only when it has both"
            ));
        }
        (false, false) if !assignments.is_empty() => {
            return Some(format!(
                "a bare `repair` diagnoses and takes no assignments; applying one is \
                 `{CASE_KEY}=… {ACTION_KEY}=…`"
            ));
        }
        _ => {}
    }
    assignments
        .iter()
        .find(|assignment| assignment.field == ACTION_KEY && !is_repair_action(&assignment.value))
        .map(|assignment| {
            format!(
                "`{}` is not a repair action; this build offers {}",
                assignment.value,
                repair_actions()
                    .map(|action| format!("`{action}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

#[derive(Debug, Subcommand)]
pub(crate) enum OrderCommand {
    /// Report this Order's complete current facts, its Items, and its calculated totals.
    Show {
        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Change this Order's facts.
    ///
    /// The assignments are one order-independent atomic mutation over the whole Order. An
    /// empty value clears an optional fact; a blank Title, Date Ordered, or Order Status is
    /// refused, and the Order ID and the technical locator cannot be assigned at all.
    /// Entering `status=ordered` with no `date_ordered` applies today's local date, and a
    /// `date_ordered` in the same command line overrides that.
    ///
    /// A fact edit never renames the Order Directory. `order align` is what does that, and
    /// it is asked for explicitly.
    Edit {
        /// The facts to assign, as `field=value`.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Print the conventional absolute path of this Order's README.
    ///
    /// The path is printed whether or not the document is presently there, so it can be
    /// composed with `cat`, `vi`, or any other ordinary tool.
    Readme {
        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    // The one `order` command that answers for a Damaged Order rather than failing on
    // one: judging an Order that cannot be read is the whole reason it exists.
    /// Validate this Order's scope.
    ///
    /// Every diagnostic in this Order's scope at once, and no sibling's. A problem with
    /// the Package around it is reported as a Package problem rather than as this Order's,
    /// because an Order-scoped answer depends on Package-wide evidence.
    Validate {
        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    // The Order-scoped half of `spielorders repair`, in the same two moods and with the
    // same assignment language. A Package-level case is the other command's; neither
    // reaches into the other's scope.
    /// Diagnose this Order's actionable repair cases, or apply one of them.
    ///
    /// With no assignments it diagnoses and changes nothing. `case=… action=…` applies
    /// exactly one action the diagnosis offered, with `field=value` resolutions where the
    /// action needs one. There is no `--fix`, no repair-all, and nothing here is inferred.
    Repair {
        /// `case=…` and `action=…`, and the resolutions the action requires.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    // The one command that renames an Order Directory. Drift is ordinary allowed state —
    // editing a fact deliberately leaves the directory alone — so this is asked for
    // explicitly and never follows an edit.
    /// Rename this Order Directory to the name its facts project.
    ///
    /// The Order stays active and its Supporting Material travels with it, intact and
    /// uninspected. A name that is already taken stops the alignment rather than being
    /// resolved with an invented suffix.
    Align {
        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    // The only command that may dispose of Supporting Material, and it disposes of it
    // only as part of one whole Order Directory. Nothing here erases: `--delete` is a
    // second destination, not a second severity.
    /// Take this whole Order Directory out of the Package.
    ///
    /// The complete directory — managed metadata, README, and opaque Supporting Material
    /// — moves into the Package's Removed Orders Directory, which is created at that
    /// moment if it is not there. Recovery is moving the directory back with ordinary
    /// tools.
    Remove {
        /// Send the directory to the platform Trash instead.
        ///
        /// It is submitted whole and nothing is erased: permanent disposal stays
        /// ordinary platform trash management. A platform with no supported Trash
        /// reports that rather than deleting anything.
        #[arg(long)]
        delete: bool,

        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Work with this Order's ordered Item collection.
    Item {
        #[command(subcommand)]
        command: ItemCommand,
    },
}

// Items are embedded in the Order that owns them and have no identities, so the three
// commands that address one take a one-based `POSITION` in that Order's ordered
// collection. `POSITION` is taken as written rather than parsed into a number, exactly as
// a Contact's is: which numbers name an Item is a fact about the collection, so all four
// ways of getting one wrong — zero, negative, not a number, past the end — are one refused
// mutation under one code rather than a usage error for two of them and a refusal for the
// others.
#[derive(Debug, Subcommand)]
pub(crate) enum ItemCommand {
    /// List this Order's Items, numbered from one, with their calculated totals.
    List {
        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Add an Item to this Order's ordered collection.
    ///
    /// The Item is appended unless `position=N` says where to insert it, in which case the
    /// Items from there on shift down. An Item requires a Description, a Quantity, and a
    /// Unit Price; its Catalog Number and multiline `notes` are optional, and its totals are
    /// calculated rather than assigned.
    Add {
        /// The Item's facts as `field=value`, and optionally `position=N`.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Change the facts of the Item at this position.
    ///
    /// The assignments are one order-independent atomic mutation over that Item. An empty
    /// value clears the optional Catalog Number or Notes; a blank Description, Quantity, or
    /// Unit Price is refused.
    Edit {
        /// The Item's one-based position in this Order's collection.
        #[arg(value_name = "POSITION", allow_negative_numbers = true)]
        position: String,

        /// The facts to assign, as `field=value`.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Move the Item at this position to another position.
    ///
    /// Both positions are evaluated against the collection as it stands before the move.
    /// Moving an Item to the position it already occupies changes nothing and is not an
    /// error.
    Move {
        /// The Item's one-based position in this Order's collection.
        #[arg(value_name = "POSITION", allow_negative_numbers = true)]
        position: String,

        /// Where the Item goes, as `position=N`.
        #[arg(value_name = "position=POSITION", value_parser = Destination::parse)]
        destination: Destination,

        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Remove the Item at this position.
    ///
    /// An Order's last Item may be removed: its Items are then a collection that is
    /// present and empty, which is what an Order created without one has.
    Remove {
        /// The Item's one-based position in this Order's collection.
        #[arg(value_name = "POSITION", allow_negative_numbers = true)]
        position: String,

        #[command(flatten)]
        target: OrderTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
}

/// One canonical `key=value` assignment on an `init` command line.
///
/// The two configuration keys are spelled as `.spielorders/config.toml` spells them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Assignment {
    pub key: &'static str,
    pub value: String,
}

impl Assignment {
    /// The keys `init` accepts.
    ///
    /// What `init` takes, not a claim about what the configuration permits.
    const KEYS: [&'static str; 2] = [ORDERS_PATH, REMOVED_PATH];

    /// Parses one assignment, splitting on the first `=` so a value may contain more.
    ///
    /// **A token that is not one of this command's assignments is a usage error**, which
    /// is why this rejects rather than deferring: `init`'s two keys are configuration the
    /// adapter must read before any core capability exists to consult, so a token naming
    /// something else has named nothing this command has — as an unrecognized option
    /// has — and nothing about the directory was attempted. A `field=value` naming a
    /// *record* field is the other thing entirely: the core adjudicates those and reports
    /// each rejected assignment in its own typed report.
    fn parse(argument: &str) -> Result<Self, String> {
        let (key, value) = argument
            .split_once('=')
            .ok_or_else(|| "an assignment is `key=value`".to_owned())?;
        let key = Self::KEYS
            .into_iter()
            .find(|candidate| *candidate == key)
            .ok_or_else(|| format!("`init` takes `{ORDERS_PATH}` and `{REMOVED_PATH}`"))?;
        Ok(Self {
            key,
            value: value.to_owned(),
        })
    }
}

/// One canonical `field=value` assignment naming a field of a record.
///
/// **The field is not checked here**, which is the whole difference from `init`'s
/// [`Assignment`]: those two keys are configuration the adapter must read before any core
/// capability exists to consult, while a record's fields are the Package's own. So an
/// unknown field, an immutable one, a technical one, and a repeated one are all refused by
/// the core, at exit `1`, each named in one typed report — and what is refused *here* is
/// only a token that is not an assignment at all.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FieldAssignment {
    pub field: String,
    pub value: String,
}

impl FieldAssignment {
    /// Parses one assignment, splitting on the first `=` so a value may contain more.
    fn parse(argument: &str) -> Result<Self, String> {
        let (field, value) = argument
            .split_once('=')
            .ok_or_else(|| "an assignment is `field=value`".to_owned())?;
        Ok(Self {
            field: field.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// One positional token of an `adopt-order` command line.
///
/// **`item` is a word rather than an assignment**, because an adopted Order may carry
/// several Items and each is its own set of the same four assignments. A field name cannot
/// say which Item it belongs to, and `items=` is the whole ordered collection, which the
/// core refuses as an assignment for the reason it refuses any collection: a mutation
/// addresses a member, never the array. So the word opens one Item and the assignments
/// after it are that Item's, which is the `order item add` command line with its verb
/// left off.
///
/// Adoption is the only mutation that may carry Items at creation — an Order created from
/// nothing starts with none — so this is the only command line that groups them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AdoptionToken {
    /// A canonical `field=value` assignment.
    Assigned(FieldAssignment),
    /// The word that opens one Order Item.
    Item,
}

impl AdoptionToken {
    fn parse(argument: &str) -> Result<Self, String> {
        if argument == ITEM_KEYWORD {
            return Ok(Self::Item);
        }
        FieldAssignment::parse(argument)
            .map(Self::Assigned)
            .map_err(|_| {
                format!("an assignment is `field=value`, and `{ITEM_KEYWORD}` opens an Order Item")
            })
    }
}

/// The word that opens one Order Item on an adoption command line.
///
/// The same noun the `order item` commands are spelled with, so one collection is named
/// one way wherever a command line mentions it.
const ITEM_KEYWORD: &str = "item";

/// The Order's own facts, and each Order Item's, as an adoption command line groups them.
///
/// **Grouping is a fact about the command line**, so it happens here and every assignment
/// is still the core's to adjudicate: which fields an Order has, which an Item has, what an
/// empty value means, and which facts each requires are Package rules. An assignment
/// written before the first `item` is the Order's; each one after an `item` belongs to that
/// Item.
///
/// Total, because [`AdoptionToken::parse`] has already refused every token that is neither.
pub(crate) fn adoption_assignments(
    tokens: &[AdoptionToken],
) -> (Vec<FieldAssignment>, Vec<Vec<FieldAssignment>>) {
    let mut facts = Vec::new();
    let mut items: Vec<Vec<FieldAssignment>> = Vec::new();
    for token in tokens {
        match token {
            AdoptionToken::Item => items.push(Vec::new()),
            AdoptionToken::Assigned(assignment) => match items.last_mut() {
                Some(item) => item.push(assignment.clone()),
                None => facts.push(assignment.clone()),
            },
        }
    }
    (facts, items)
}

/// One positional token of a creating command line, before the two forms are told apart.
///
/// The bare-word shorthand and the assignment form are one variadic positional rather than
/// two arguments, because a parser given `NAME` and `ASSIGNMENT...` separately would take
/// `name=Tarnwick` for the Name. Which form a command line is in follows from whether its
/// tokens are assignments, and [`parse`] refuses one that is in both.
///
/// **One type for both creating commands.** `add-vendor "Tarnwick"` and
/// `add-order "Laser components"` are one shape over two different subjects, and the whole
/// difference is which field the bare word stands for — which [`subject_assignments`]
/// takes as an argument. Two token types would be two places for the rule that the forms
/// cannot be combined.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SubjectToken {
    /// A canonical `field=value` assignment.
    Assigned(FieldAssignment),
    /// A bare word: the shorthand.
    Named(String),
}

impl SubjectToken {
    /// Total, because a token either is an assignment or is not.
    fn parse(argument: &str) -> Result<Self, String> {
        Ok(match FieldAssignment::parse(argument) {
            Ok(assignment) => Self::Assigned(assignment),
            Err(_) => Self::Named(argument.to_owned()),
        })
    }
}

/// The assignment set a creating command line supplies.
///
/// **The shorthand is expanded rather than handled separately**, which is what makes
/// "both forms produce the same entity" true by construction instead of by two code paths
/// agreeing: `add-vendor "Tarnwick"` *is* `add-vendor name=Tarnwick`, and the Name reaches
/// the core as one assignment either way.
///
/// `field` is the record's own field name, which is also what the core adjudicates and what
/// a refusal names — so the shorthand and the assignment it stands for are refused
/// identically.
///
/// Total, because [`parse`] has already refused every command line the two forms could not
/// be told apart on — a bare word beside an assignment, or two bare words.
pub(crate) fn subject_assignments(subject: &[SubjectToken], field: &str) -> Vec<FieldAssignment> {
    subject
        .iter()
        .map(|token| match token {
            SubjectToken::Assigned(assignment) => assignment.clone(),
            SubjectToken::Named(value) => FieldAssignment {
                field: field.to_owned(),
                value: value.clone(),
            },
        })
        .collect()
}

/// The field `add-vendor`'s shorthand assigns: the Vendor record's own Name.
pub(crate) const VENDOR_NAME_FIELD: &str = "name";

/// The field `add-order`'s shorthand assigns: the Order record's own Title.
pub(crate) const ORDER_TITLE_FIELD: &str = "title";

/// What a mutating command may be asked to report rather than do.
///
/// Flattened into each mutating command rather than declared globally, because a read
/// has no mutation to withhold: `package show --dry-run` would be an option answering a
/// question that command does not ask.
#[derive(Debug, Args)]
pub(crate) struct Execution {
    /// Validate and report the whole mutation without performing any of it.
    ///
    /// No directory, file, staging artifact, index write, or cleanup effect is produced.
    /// There is no confirmation prompt and no `--yes`: naming the mutation and leaving
    /// this option off is the authorization.
    #[arg(long)]
    pub dry_run: bool,
}

/// The SpielOrders Root a Package-level command acts on.
///
/// One of the two targeting groups, and the difference between them is the whole rule:
/// a command carrying this one takes the SpielOrders Root, and a command carrying
/// [`OrderTarget`] takes the Order Directory. Which root a command takes is therefore
/// the command's own type rather than a sentence a reader has to remember.
#[derive(Debug, Args)]
pub(crate) struct PackageTarget {
    /// The SpielOrders Root, named exactly. No ancestor is ever searched.
    #[arg(long, value_name = "PATH", default_value = ".")]
    pub root: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct DirectoryTarget {
    /// The directory to provision, named exactly. It need not be a Package, and no
    /// ancestor is ever searched.
    #[arg(long, value_name = "PATH", default_value = ".")]
    pub root: PathBuf,
}

/// The words an entity search looks for.
///
/// One group attached to both entity searches, so the terms are declared once and the two
/// commands cannot come to accept them differently.
#[derive(Debug, Args)]
pub(crate) struct Keywords {
    /// The words to look for, combined with AND.
    ///
    /// Whitespace separates terms wherever it stands, so one quoted multiword argument
    /// searches as several terms and matches exactly what those terms match when the
    /// shell splits them. Each term matches a substring of one searchable fact, and two
    /// terms may land on two different facts of one entity. The text is literal
    /// throughout: never a phrase, and never a regular expression.
    ///
    /// With no words at all the query applies no text restriction.
    #[arg(value_name = "KEYWORD")]
    pub keywords: Vec<String>,
}

/// How an entity search compares letter case.
///
/// One group for the same reason [`Keywords`] is one. There is nothing else in it: the
/// query's other rules — whitespace-separated terms combined with AND, substring
/// matching, and the fold that keeps a diacritic from distinguishing — are the query's own
/// and no option governs any of them.
#[derive(Debug, Args)]
pub(crate) struct Matching {
    /// Distinguish letter case.
    ///
    /// Without it, matching is case-insensitive. Nothing else about the query changes.
    #[arg(long)]
    pub case_sensitive: bool,
}

/// The Order an `order` command acts on, named by its Directory or by its identity.
///
/// **The exclusion is declared on the group**, which is the whole of it: both ways of
/// naming one Order stand at one parse level, so the parser refuses a command line that
/// names both and nothing downstream considers an Order named twice.
#[derive(Debug, Args)]
#[group(multiple = false)]
pub(crate) struct OrderTarget {
    /// The Order Directory, named exactly. No ancestor is ever searched.
    ///
    /// The Package is reached from there through the Order's own technical locator.
    #[arg(long, value_name = "PATH", default_value = ".")]
    pub root: PathBuf,

    // Provisional, reserved for GUI and scripting evaluation during MVP, and it may be
    // removed before the CLI contract is declared stable. Human workflows use Order
    // Directory paths, never UUIDs and never non-unique Order Titles.
    /// Resolve the Order by its Order ID against the current-directory Package.
    ///
    /// Cannot be combined with `--root`: the two are different ways of naming the same
    /// Order, and a command line that supplied both would be asking about two.
    #[arg(long, value_name = "ORDER_ID")]
    pub uuid: Option<String>,
}

/// Which of the four decided output modes a command reports in.
///
/// **The exclusion is declared on the group**, so it is written once and enforced by the
/// parser wherever the group is attached. Default human output is the absence of all
/// three rather than a fourth flag, so it excludes nothing and needs no member here.
#[derive(Debug, Args)]
#[group(multiple = false)]
pub(crate) struct OutputMode {
    /// Write one versioned machine-readable JSON document on standard output.
    #[arg(long)]
    pub json: bool,

    /// Say nothing on success. A failure is still reported on standard error.
    #[arg(long)]
    pub quiet: bool,

    /// Add human detail to the result.
    #[arg(long)]
    pub verbose: bool,
}

impl OutputMode {
    /// The one selected output mode.
    ///
    /// At most one of the three flags can be set, because the group they belong to admits
    /// one member and the parser refuses a command line that names two.
    pub(crate) fn mode(&self) -> Mode {
        match (self.json, self.quiet, self.verbose) {
            (true, _, _) => Mode::Json,
            (_, true, _) => Mode::Quiet,
            (_, _, true) => Mode::Verbose,
            (false, false, false) => Mode::Human,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum IndexCommand {
    /// Report what state the derived Package Index is in.
    ///
    /// Whether it is current, missing, malformed, at a version this build does not
    /// project, or stale, each with its own typed reasons. It writes nothing — asking
    /// what state the index is in must not be the thing that changes it.
    Status {
        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Replace the derived Package Index from the healthy canonical records.
    ///
    /// Deterministic: the same healthy Package rebuilds to identical bytes. A Damaged
    /// Order is absent from the result, because its record could not be read and it has
    /// nothing to project; it stays visible in the diagnostics. This is not a repair for
    /// canonical damage and it never becomes one.
    Rebuild {
        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        execution: Execution,

        #[command(flatten)]
        output: OutputMode,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PackageCommand {
    /// Print the conventional absolute path of the Package README.
    Readme {
        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    /// Report everything needed to open this Package.
    Show {
        // A presentation option, not a domain fact and not a core filter. The core
        // exposes both the complete Vendor list with visibility and the offered-only
        // list; which one a read presents is the CLI's choice, and the decided default is
        // the Vendors this Package offers for new use.
        /// Report Hidden Vendors too, each carrying its visibility.
        ///
        /// Without it, the Vendors this Package offers for new use are reported. An
        /// Order that references a Hidden Vendor still reports that Vendor's facts
        /// either way: hiding governs what is offered, never what is resolved.
        #[arg(long)]
        all: bool,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    // A collection belongs to the Package that holds it, and an Order-rooted command that
    // listed the Package's Orders would target one Order to report its siblings.
    /// List one of this Package's collections.
    ///
    /// A Package's collections are listed here, so there is no `order list`; every
    /// `order` command targets one Order Directory.
    List {
        #[command(subcommand)]
        command: ListCommand,
    },
    // One command taking a kind rather than four sibling commands, because the four
    // answers have one shape and differ only in which list they are about. Each kind
    // still owns its own independently versioned response contract.
    /// Report one Reusable Free-Entry Value list with its usage counts.
    Values {
        /// Which Reusable Free-Entry Value list to report.
        kind: ValueKind,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ListCommand {
    // No option governs either of the two facts below: a listing that omitted a Damaged
    // Order would say the Order is gone, and an Order's Vendor is resolved rather than
    // offered, so Vendor visibility never governs it.
    /// List this Package's Orders, newest Date Ordered first.
    ///
    /// An Order whose record could not be read is listed as a placeholder rather than
    /// omitted, and each Order names the Vendor it references whether or not that
    /// Vendor is hidden.
    Orders {
        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
    /// List the Vendors this Package offers for new use.
    Vendors {
        // The same presentation option `package show` carries, with the same default.
        // The core exposes both the complete Vendor list and the offered-only one and
        // filters neither; which a read presents is the CLI's choice, and these two
        // commands make it identically rather than each inventing a convention.
        /// Report Hidden Vendors too, each carrying its visibility.
        ///
        /// Without it, the Vendors this Package offers for new use are reported —
        /// the same default `package show` uses.
        #[arg(long)]
        all: bool,

        #[command(flatten)]
        target: PackageTarget,

        #[command(flatten)]
        output: OutputMode,
    },
}

/// One of the four Reusable Free-Entry Value categories.
///
/// **Each spelling is named explicitly rather than derived**, because it is the `<kind>`
/// in the `spielorders.package.values.<kind>` contract identity and in that contract's
/// fixture directory. It is the Order record's own field name — the same token a
/// `field=value` assignment will carry — so one Reusable Free-Entry Value category is
/// spelled one way across the command line, the response contract, and the mutations
/// that come later.
///
/// **The declaration order is `package show`'s published `values` payload order**, which
/// is the only one of the candidate orders a shipped contract already fixes. It is
/// therefore also the order of `--help`'s possible values, of `VALUE_KINDS`, and of
/// `package show`'s own human summary — one order for the four categories everywhere,
/// rather than a published one and a presented one for a reader to reconcile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ValueKind {
    #[value(name = "ordering_institution")]
    OrderingInstitution,
    #[value(name = "orderer")]
    Orderer,
    #[value(name = "project")]
    Project,
    #[value(name = "order_group")]
    OrderGroup,
}

/// Parses the command line, leaving every byte of output to the caller.
///
/// The parser is told to style its own rendered text unconditionally, and this crate
/// then renders it styled or plain according to one color policy — see
/// [`crate::output::color_permitted`]. Two color policies, one in the parser and one in
/// this crate, is how a `NO_COLOR` request comes to be honored in half a program.
pub(crate) fn parse(arguments: &[OsString]) -> Result<Cli, clap::Error> {
    let mut command = Cli::command().color(clap::ColorChoice::Always);
    let matches = command.try_get_matches_from_mut(arguments)?;
    let cli = Cli::from_arg_matches(&matches)?;
    if let Family::Init { assignments, .. } = &cli.family
        && let Some(repeated) = repeated_assignment(assignments)
    {
        // One assignment set, one value per key. Letting the last one win would make a
        // command line that says two things do one of them silently, which is the same
        // refusal two output modes at once already gets.
        return Err(command.error(
            ErrorKind::ValueValidation,
            format!("`{repeated}` is assigned twice"),
        ));
    }
    let creating = match &cli.family {
        Family::AddVendor { subject, .. } => Some(("add-vendor", "Vendor", "Name", subject)),
        Family::AddOrder { subject, .. } => Some(("add-order", "Order", "Title", subject)),
        _ => None,
    };
    if let Some((command_name, entity, shorthand, subject)) = creating
        && let Some(message) = combined_subject_forms(command_name, entity, shorthand, subject)
    {
        // `ValueValidation` rather than `ArgumentConflict`, which the parser raises for the
        // exclusions declared on an option group.
        return Err(command.error(ErrorKind::ValueValidation, message));
    }
    let inserting = match &cli.family {
        Family::Vendor {
            command:
                VendorCommand::Contact {
                    command: ContactCommand::Add { assignments, .. },
                },
        } => Some(assignments),
        Family::Order {
            command:
                OrderCommand::Item {
                    command: ItemCommand::Add { assignments, .. },
                },
            ..
        } => Some(assignments),
        _ => None,
    };
    if let Some(assignments) = inserting
        && assignments
            .iter()
            .filter(|assignment| assignment.field == POSITION_KEY)
            .count()
            > 1
    {
        // One insertion point, for the reason `init` takes one value per configured key:
        // letting the last one win would make a command line that says two things do one
        // of them silently. A repeated fact of the member itself is not checked here — that
        // is the core's refusal, reported with every other refused assignment of the set.
        return Err(command.error(
            ErrorKind::ValueValidation,
            format!("`{POSITION_KEY}` is assigned twice"),
        ));
    }
    let repairing = match &cli.family {
        Family::Repair { assignments, .. } => Some(assignments),
        Family::Order {
            command: OrderCommand::Repair { assignments, .. },
            ..
        } => Some(assignments),
        _ => None,
    };
    if let Some(assignments) = repairing
        && let Some(message) = incomplete_repair_request(assignments)
    {
        return Err(command.error(ErrorKind::ValueValidation, message));
    }
    Ok(cli)
}

/// Why this creating command line is in neither form, where it is in neither.
///
/// The two forms cannot be combined, so a bare word beside an assignment names the subject
/// two ways at once and is refused rather than resolved by a precedence nobody decided. Two
/// bare words are the same refusal: one command creates one entity, so a second bare word
/// is not a second subject.
///
/// One function for both creating commands, so the rule is stated once and each command
/// supplies only the three nouns its sentence needs.
///
/// A repeated *field* is not checked here. It is the core's refusal, reported with every
/// other refused assignment of the same set — see [`FieldAssignment`].
fn combined_subject_forms(
    command: &str,
    entity: &str,
    shorthand: &str,
    subject: &[SubjectToken],
) -> Option<String> {
    let named = subject
        .iter()
        .filter(|token| matches!(token, SubjectToken::Named(_)))
        .count();
    match (named, subject.len()) {
        (0, _) | (1, 1) => None,
        (1, _) => Some(format!(
            "a {shorthand} and `field=value` assignments cannot be combined"
        )),
        _ => Some(format!(
            "one `{command}` creates one {entity}, so it takes one {shorthand}"
        )),
    }
}

/// The first key assigned more than once, where a command line assigns one twice.
fn repeated_assignment(assignments: &[Assignment]) -> Option<&'static str> {
    assignments
        .iter()
        .enumerate()
        .find(|(index, assignment)| {
            assignments[..*index]
                .iter()
                .any(|earlier| earlier.key == assignment.key)
        })
        .map(|(_, assignment)| assignment.key)
}

/// Whether the parse error is a request for generated reference text rather than a
/// usage error. `--help` and `--version` are answers, not failures.
pub(crate) fn is_reference_request(error: &clap::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    )
}

/// Whether a machine-readable response was requested, answered from the raw arguments.
///
/// A usage error means parsing produced no options to read, and the contract still
/// requires that a `--json` caller receive one JSON document rather than generated
/// human help. This is that answer, and it is the only question ever asked of the raw
/// arguments — only when parsing has already failed. It errs toward the machine: a
/// caller that named `--json` anywhere gets a document, because a script is worse
/// served by help text than a human is by JSON.
pub(crate) fn json_requested(arguments: &[OsString]) -> bool {
    arguments.iter().any(|argument| argument == "--json")
}

/// One stable sentence for a usage error, with no generated help in it.
///
/// The one condition whose wording is chosen here is an unrecognized option, so a
/// consumer reads the same sentence for it across parser releases. Anything else falls
/// back to the parser's own statement of the problem — including every refusal this
/// module composes itself, whose whole message is that statement. The rest of what the
/// parser renders is a tip, the usage block, and a pointer to `--help`, and that belongs
/// on standard error in human mode rather than inside a response document.
pub(crate) fn usage_message(error: &clap::Error) -> String {
    let named = |kind: ContextKind| match error.get(kind) {
        Some(ContextValue::String(value)) => Some(value.clone()),
        Some(ContextValue::Strings(values)) => Some(values.join(", ")),
        _ => None,
    };
    match error.kind() {
        ErrorKind::UnknownArgument => match named(ContextKind::InvalidArg) {
            Some(argument) => format!("unrecognized option `{argument}`"),
            None => "unrecognized option".to_owned(),
        },
        _ => terse(error),
    }
}

/// The parser's own statement of the problem, on one line.
///
/// **The whole of it, not its first line.** The statement is what the parser renders
/// before its first blank line. Most conditions are one line, and two are not: a conflict
/// among three options names the first and lists the rest beneath it, and an invalid value
/// for a fixed-choice argument appends the values it would have accepted. Taking only the
/// first line would put a sentence ending in a colon into a response document, and would
/// drop the one part of an invalid-value refusal a caller can act on.
fn terse(error: &clap::Error) -> String {
    let rendered = error.render().to_string();
    let statement = rendered
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    statement
        .strip_prefix("error: ")
        .unwrap_or(&statement)
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    // Decision sources: Define the Rust CLI capability contract.

    fn arguments(rest: &[&str]) -> Vec<OsString> {
        std::iter::once(OsString::from("spielorders"))
            .chain(rest.iter().map(OsString::from))
            .collect()
    }

    /// Every set of options one command line may name at most one of.
    ///
    /// **Read out of the parser's own groups rather than transcribed**, so this is what the
    /// grammar is rather than a second statement of it that can disagree: an exclusion is
    /// declared on the group that owns it — see [`OutputMode`] and [`OrderTarget`] — and a
    /// group that admits one member is exactly one of these. The same group is attached to
    /// every command that takes it, so a set is reported once however many commands declare
    /// it.
    ///
    /// Each member is the parser's own argument id, which is what a caller writes with `--`
    /// in front of it for every option this command line has.
    fn exclusions() -> Vec<Vec<String>> {
        let mut found = BTreeSet::new();
        collect_exclusions(&Cli::command(), &mut found);
        found.into_iter().collect()
    }

    fn collect_exclusions(command: &clap::Command, found: &mut BTreeSet<Vec<String>>) {
        for group in command.get_groups() {
            // `is_multiple` wants the group by value, and a group is only ever read here.
            if !group.clone().is_multiple() {
                let members: Vec<String> = group.get_args().map(ToString::to_string).collect();
                if members.len() > 1 {
                    found.insert(members);
                }
            }
        }
        for subcommand in command.get_subcommands() {
            collect_exclusions(subcommand, found);
        }
    }

    /// The command every exclusion this executable declares is reachable on.
    ///
    /// One command line can name every option of every exclusion, so each pair below is
    /// written where a caller can write it. A group declared somewhere this command does
    /// not carry fails in [`written`] by name rather than being quietly skipped.
    fn every_excluded_option() -> clap::Command {
        Cli::command()
            .find_subcommand("order")
            .and_then(|family| family.find_subcommand("show"))
            .expect("`order show` is a command")
            .clone()
    }

    /// One option as a caller writes it: its long form, and a value where it takes one.
    ///
    /// Asked of the parser rather than written down beside the exclusion, so an option
    /// that comes to take a value is still written correctly here.
    fn written(command: &clap::Command, option: &str) -> Vec<String> {
        let argument = command
            .get_arguments()
            .find(|argument| argument.get_id().as_str() == option)
            .expect("every excluded option is one `order show` declares");
        let mut tokens = vec![format!("--{option}")];
        if argument.get_action().takes_values() {
            tokens.push("value".to_owned());
        }
        tokens
    }

    #[test]
    fn the_default_target_is_the_working_directory() {
        let cli = parse(&arguments(&["package", "readme"])).expect("the command parses");
        assert_eq!(cli.mode(), Mode::Human);
        let Family::Package {
            command: PackageCommand::Readme { target, .. },
        } = cli.family
        else {
            panic!("the command line named `package readme`")
        };
        assert_eq!(target.root, PathBuf::from("."));
    }

    #[test]
    fn each_output_mode_is_selected_on_its_own() {
        for (flag, mode) in [
            ("--json", Mode::Json),
            ("--quiet", Mode::Quiet),
            ("--verbose", Mode::Verbose),
        ] {
            let cli = parse(&arguments(&["package", "readme", flag])).expect("the mode parses");
            assert_eq!(cli.mode(), mode, "{flag}");
        }
    }

    /// **Every excluded pair, derived from the group that declares the exclusion.**
    ///
    /// The defect each of these ends is a silent precedence: two options given and one
    /// quietly winning, so a script that asked for `--json` and got human text because
    /// `--quiet` came later would be parsing prose with nothing having said so.
    ///
    /// One placement, because the grammar allows one: each option is declared on the
    /// command it modifies, so a pair a caller can write is a pair the parser sees
    /// together. An option added to one of these groups is swept here without anyone
    /// remembering that this test is where that is asserted.
    /// The parser's groups are the decided exclusions: the output modes, and the two ways
    /// an `order` command names one Order.
    #[test]
    fn the_declared_exclusions_are_the_decided_ones() {
        assert_eq!(
            exclusions(),
            [vec!["json", "quiet", "verbose"], vec!["root", "uuid"]]
        );
    }

    #[test]
    fn two_options_of_one_exclusion_are_refused_rather_than_ordered() {
        let command = every_excluded_option();
        for exclusion in exclusions() {
            for (index, first) in exclusion.iter().enumerate() {
                for second in &exclusion[index + 1..] {
                    let mut line = vec!["order".to_owned(), "show".to_owned()];
                    line.extend(written(&command, first));
                    line.extend(written(&command, second));
                    let rest: Vec<&str> = line.iter().map(String::as_str).collect();

                    let error = parse(&arguments(&rest)).expect_err("one of the two, or neither");
                    assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{line:?}");
                    let message = usage_message(&error);
                    assert!(
                        message.contains(&format!("--{first}"))
                            && message.contains(&format!("--{second}")),
                        "{line:?}: the refusal names both options: {message}"
                    );
                }
            }
        }
    }

    /// A repeated option is two occurrences at one parse level, which the parser already
    /// refuses — so nothing in this module says anything about repetition.
    ///
    /// It was sayable at all only while an option stood in any gap between the command
    /// path's segments: one occurrence recorded at each of two levels is not a repetition
    /// to a parser that resolves one level at a time.
    #[test]
    fn an_option_written_twice_is_refused() {
        for line in [
            vec!["order", "show", "--json", "--json"],
            vec!["order", "show", "--root", "here", "--root", "there"],
        ] {
            let error = parse(&arguments(&line)).expect_err("one option, one occurrence");
            assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{line:?}");
        }
    }

    #[test]
    fn an_unrecognized_option_is_named_in_the_usage_message() {
        let error =
            parse(&arguments(&["package", "readme", "--jsom"])).expect_err("the option is unknown");
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
        assert_eq!(usage_message(&error), "unrecognized option `--jsom`");
    }

    /// A refused fixed-choice value carries the values that would have been accepted.
    ///
    /// The parser renders them after the clause, without a blank line, so they are part of
    /// its statement of the problem rather than the help that follows it. They are the one
    /// part of this refusal a caller can act on, and the usage error's `detail` is where a
    /// caller reads it.
    #[test]
    fn a_refused_fixed_choice_value_names_what_would_have_been_accepted() {
        let error = parse(&arguments(&["package", "values", "bogus"]))
            .expect_err("`bogus` is not one of the value kinds");
        let message = usage_message(&error);
        assert!(
            message.starts_with("invalid value 'bogus' for "),
            "{message}"
        );
        assert!(
            message
                .contains("[possible values: ordering_institution, orderer, project, order_group]"),
            "{message}"
        );
    }

    /// A usage message never carries the generated usage block or the pointer to
    /// `--help`: that text is for a human on standard error, and inside a response
    /// document it is the stream contamination the contract forbids.
    #[test]
    fn no_usage_message_carries_generated_help() {
        for rest in [
            vec!["package", "readme", "--jsom"],
            vec!["package", "readme", "--json", "--quiet"],
            // The one refusal the parser renders over several lines: it names the first
            // option and lists the rest of the exclusion beneath it.
            vec!["package", "readme", "--json", "--quiet", "--verbose"],
            vec!["package", "reamde"],
            vec![],
        ] {
            let error = parse(&arguments(&rest)).expect_err("the command line is wrong");
            let message = usage_message(&error);
            assert!(!message.contains('\n'), "{rest:?}: {message}");
            assert!(!message.contains("Usage:"), "{rest:?}: {message}");
            assert!(!message.contains("--help"), "{rest:?}: {message}");
            assert!(!message.is_empty(), "{rest:?}");
        }
    }

    #[test]
    fn help_and_version_are_reference_requests_rather_than_usage_errors() {
        for flag in ["--help", "--version"] {
            let error = parse(&arguments(&[flag])).expect_err("the parser reports these as errors");
            assert!(is_reference_request(&error), "{flag}");
        }
        let error = parse(&arguments(&["package", "readme", "--jsom"])).unwrap_err();
        assert!(!is_reference_request(&error));
    }

    #[test]
    fn a_machine_response_is_recognized_in_raw_arguments() {
        assert!(json_requested(&arguments(&["package", "readme", "--json"])));
        assert!(json_requested(&arguments(&["--json", "package", "reamde"])));
        assert!(!json_requested(&arguments(&["package", "readme"])));
        assert!(!json_requested(&arguments(&[
            "package", "readme", "--jsom"
        ])));
    }
}
