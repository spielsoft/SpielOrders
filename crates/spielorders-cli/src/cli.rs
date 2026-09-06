//! The command line: Git-style first-level families, their commands, and the global
//! options that govern targeting, output, and execution.
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
    ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum,
    error::{ContextKind, ContextValue, ErrorKind},
    parser::ValueSource,
};
use spielorders_core::{readme::ConfigurationPathKind, repair_cases::RepairAction};
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

    #[command(flatten)]
    pub options: GlobalOptions,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Family {
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
        execution: Execution,
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
        execution: Execution,
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
        execution: Execution,
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
        execution: Execution,
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
    Validate,
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
        execution: Execution,
    },
    // A first-level command with no subcommand, because there is one thing to do to a
    // Package whose metadata is behind this build: carry it to the newest version this
    // build understands. There is no target to choose — an arbitrary target, a per-file
    // upgrade, a downgrade, and a package-wide rollback do not exist.
    /// Carry this Package's metadata to the newest version this build understands.
    ///
    /// The complete supported sequential Metadata Upgrade, resumed by rerunning it. Valid
    /// artifacts are carried one at a time and stay carried across an interruption; the
    /// Package marker advances only after every artifact has.
    Upgrade {
        #[command(flatten)]
        execution: Execution,
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

        #[command(flatten)]
        targeting: OrderTargeting,
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
}

#[derive(Debug, Subcommand)]
pub(crate) enum VendorCommand {
    /// Report this Vendor's current facts, its Contacts numbered from one, and the
    /// Orders that reference it.
    Show {
        /// The Vendor Name.
        name: String,
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
        execution: Execution,
    },
    // A Vendor is never deleted, so there is no `vendor remove`. These two are what
    // replaced deletion: they govern whether a Vendor is offered for new use, never
    // whether it resolves, so neither is refused on reference grounds and either can be
    // undone by the other.
    /// Stop offering this Vendor for new use.
    ///
    /// Its record and its Vendor ID stay where they are, so every Order that references
    /// it keeps resolving its current facts.
    Hide {
        /// The Vendor Name.
        name: String,

        #[command(flatten)]
        execution: Execution,
    },
    /// Offer this Vendor for new use again.
    Unhide {
        /// The Vendor Name.
        name: String,

        #[command(flatten)]
        execution: Execution,
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
        execution: Execution,
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
        execution: Execution,
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
        execution: Execution,
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
        execution: Execution,
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
/// Item has a Description, a Quantity, a Unit Price, and a Catalog Number, and where either
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
        action: RepairAction,
        /// Every assignment that is not `case` or `action`. Which of them the action needs
        /// is the core's answer, asked of [`RepairAction::required_values`].
        resolutions: Vec<FieldAssignment>,
    },
}

/// The request one `repair` command line carries.
///
/// **The action's spelling is the core's**, resolved through [`RepairAction::parse`] so
/// this adapter never carries a second table of those words.
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
    let Some(action) = RepairAction::parse(action) else {
        return RepairRequest::Diagnose;
    };
    RepairRequest::Apply {
        case: case.to_owned(),
        action,
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
        .find(|assignment| {
            assignment.field == ACTION_KEY && RepairAction::parse(&assignment.value).is_none()
        })
        .map(|assignment| {
            format!(
                "`{}` is not a repair action; this build offers {}",
                assignment.value,
                RepairAction::ALL
                    .iter()
                    .map(|action| format!("`{}`", action.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

#[derive(Debug, Subcommand)]
pub(crate) enum OrderCommand {
    /// Report this Order's complete current facts, its Items, and its calculated totals.
    Show,
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
        execution: Execution,
    },
    /// Print the conventional absolute path of this Order's README.
    ///
    /// The path is printed whether or not the document is presently there, so it can be
    /// composed with `cat`, `vi`, or any other ordinary tool.
    Readme,
    // The one `order` command that answers for a Damaged Order rather than failing on
    // one: judging an Order that cannot be read is the whole reason it exists.
    /// Validate this Order's scope.
    ///
    /// Every diagnostic in this Order's scope at once, and no sibling's. A problem with
    /// the Package around it is reported as a Package problem rather than as this Order's,
    /// because an Order-scoped answer depends on Package-wide evidence.
    Validate,
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
        execution: Execution,
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
        execution: Execution,
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
        execution: Execution,
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
    List,
    /// Add an Item to this Order's ordered collection.
    ///
    /// The Item is appended unless `position=N` says where to insert it, in which case the
    /// Items from there on shift down. An Item requires a Description, a Quantity, and a
    /// Unit Price; its Catalog Number is optional, and its totals are calculated rather
    /// than assigned.
    Add {
        /// The Item's facts as `field=value`, and optionally `position=N`.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        execution: Execution,
    },
    /// Change the facts of the Item at this position.
    ///
    /// The assignments are one order-independent atomic mutation over that Item. An empty
    /// value clears the optional Catalog Number; a blank Description, Quantity, or Unit
    /// Price is refused.
    Edit {
        /// The Item's one-based position in this Order's collection.
        #[arg(value_name = "POSITION", allow_negative_numbers = true)]
        position: String,

        /// The facts to assign, as `field=value`.
        #[arg(value_name = "ASSIGNMENT", value_parser = FieldAssignment::parse)]
        assignments: Vec<FieldAssignment>,

        #[command(flatten)]
        execution: Execution,
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
        execution: Execution,
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
        execution: Execution,
    },
}

/// One canonical `key=value` assignment on an `init` command line.
///
/// **The two configuration keys are the core's own [`ConfigurationPathKind`]**, so one
/// configured key is spelled one way on the command line, in `.spielorders/config.toml`,
/// in a loaded Package's diagnostics, and in a refused initialization's response. A
/// second spelling here is how a key comes to be accepted under one name and reported
/// under another.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Assignment {
    pub key: ConfigurationPathKind,
    pub value: String,
}

impl Assignment {
    /// The keys `init` accepts.
    ///
    /// The core's two [`ConfigurationPathKind`] values, listed because an array cannot be
    /// derived from an enum. A third configured key added to that enum is offered by this
    /// command only once it is added here, which is the honest reading of this list: it
    /// is what `init` takes, not a claim about what the configuration permits.
    const KEYS: [ConfigurationPathKind; 2] = [
        ConfigurationPathKind::OrdersPath,
        ConfigurationPathKind::RemovedPath,
    ];

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
            .find(|candidate| candidate.as_str() == key)
            .ok_or_else(|| {
                format!(
                    "`init` takes `{}` and `{}`",
                    ConfigurationPathKind::OrdersPath.as_str(),
                    ConfigurationPathKind::RemovedPath.as_str()
                )
            })?;
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

/// The second way an `order` command can name its Order.
///
/// Flattened into the `order` family rather than declared globally, because it targets an
/// Order and nothing else does: `package show --uuid` would be an option answering a
/// question that command does not ask. It is `global` *within* this family so that it may
/// be written before or after the command, as `--root` may be.
#[derive(Debug, Args)]
pub(crate) struct OrderTargeting {
    // Provisional, reserved for GUI and scripting evaluation during MVP, and it may be
    // removed before the CLI contract is declared stable. Human workflows use Order
    // Directory paths, never UUIDs and never non-unique Order Titles.
    /// Resolve the Order by its Order ID against the current-directory Package.
    ///
    /// Cannot be combined with `--root`: the two are different ways of naming the same
    /// Order, and a command line that supplied both would be asking about two.
    // The exclusion that sentence promises is decided in `parse` over the whole command
    // line rather than declared here. See `GLOBAL_EXCLUSIONS`.
    #[arg(long, value_name = "ORDER_ID", global = true)]
    pub uuid: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum IndexCommand {
    /// Report what state the derived Package Index is in.
    ///
    /// Whether it is current, missing, malformed, at a version this build does not
    /// project, or stale, each with its own typed reasons. It writes nothing — asking
    /// what state the index is in must not be the thing that changes it.
    Status,
    /// Replace the derived Package Index from the healthy canonical records.
    ///
    /// Deterministic: the same healthy Package rebuilds to identical bytes. A Damaged
    /// Order is absent from the result, because its record could not be read and it has
    /// nothing to project; it stays visible in the diagnostics. This is not a repair for
    /// canonical damage and it never becomes one.
    Rebuild {
        #[command(flatten)]
        execution: Execution,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PackageCommand {
    /// Print the conventional absolute path of the Package README.
    Readme,
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
    Orders,
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
pub enum ValueKind {
    #[value(name = "ordering_institution")]
    OrderingInstitution,
    #[value(name = "orderer")]
    Orderer,
    #[value(name = "project")]
    Project,
    #[value(name = "order_group")]
    OrderGroup,
}

#[derive(Debug, Args)]
pub(crate) struct GlobalOptions {
    /// The root of the command's subject, named exactly. No ancestor is ever searched.
    ///
    /// The SpielOrders Root for a Package-level or `vendor` command — Vendors have no
    /// directories — and the Order Directory for an `order` command.
    #[arg(long, value_name = "PATH", default_value = ".", global = true)]
    pub root: PathBuf,

    // The three modes exclude each other, and that exclusion is decided in `parse` over
    // the whole command line rather than declared on each of them. See
    // `GLOBAL_EXCLUSIONS`.
    /// Write one versioned machine-readable JSON document on standard output.
    #[arg(long, global = true)]
    pub json: bool,

    /// Say nothing on success. A failure is still reported on standard error.
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Add human detail to the result.
    #[arg(long, global = true)]
    pub verbose: bool,
}

impl GlobalOptions {
    /// The one selected output mode.
    ///
    /// At most one of the three flags can be set, because a command line that named two
    /// is refused as a usage error before this is asked rather than letting one silently
    /// take precedence. See `GLOBAL_EXCLUSIONS`.
    pub(crate) fn mode(&self) -> Mode {
        match (self.json, self.quiet, self.verbose) {
            (true, _, _) => Mode::Json,
            (_, true, _) => Mode::Quiet,
            (_, _, true) => Mode::Verbose,
            (false, false, false) => Mode::Human,
        }
    }
}

/// Every set of global options a command line may name at most one of.
///
/// **The exclusion is decided over the whole command line rather than declared on the
/// options**, because a `conflicts_with` rule is resolved against one parse level's own
/// matches and a global option stands in any gap between the command path's segments. A
/// pair written on either side of a subcommand therefore lands in two sets of matches and
/// such a rule never sees them together — which makes the caller's typing decide whether
/// the exclusion applies, and an exclusion a caller can write around is not one. See
/// [`excluded_globals`], which decides these in [`parse`] where no boundary exists.
///
/// Each entry is the parser's own argument id. A refusal names the two in the order they
/// stand here, so one conflict reads as one sentence however the caller wrote it.
pub const GLOBAL_EXCLUSIONS: &[&[&str]] = &[
    // The four decided output modes. Default human output is the absence of all three, so
    // it excludes nothing and is not one of them here.
    &["json", "quiet", "verbose"],
    // The two ways of naming one Order — see `OrderTargeting`. `--uuid` belongs to the
    // `order` family and `--root` to the executable, which is exactly the boundary a
    // declared conflict could not see across.
    &["root", "uuid"],
];

/// Why this command line names two options of one exclusion, where it names two.
///
/// The first exclusion with two of its options named is the one reported, and the first
/// two of those: a command line wrong in more ways than one gets one stable sentence.
fn excluded_globals(command: &clap::Command, matches: &ArgMatches) -> Option<String> {
    GLOBAL_EXCLUSIONS.iter().find_map(|exclusion| {
        let mut named = exclusion
            .iter()
            .filter(|option| named_by_the_caller(matches, option));
        match (named.next(), named.next()) {
            (Some(first), Some(second)) => Some(format!(
                "`{}` and `{}` are mutually exclusive",
                spelled(command, first),
                spelled(command, second)
            )),
            _ => None,
        }
    })
}

/// Whether the caller wrote this option, anywhere on the command line.
///
/// **Every level of the parse is asked**, which is the whole point: a global option is
/// recorded at the level it was written at, so a question put to one of them is exactly
/// the boundary this exclusion exists to ignore.
///
/// A default value is not a caller writing one, which is what tells `--root` left off
/// apart from `--root .` written out.
fn named_by_the_caller(matches: &ArgMatches, option: &str) -> bool {
    // Asked through the fallible accessor: an option one family declares is not an
    // argument of the levels above that family, and putting the question to one of them
    // directly is a panic rather than a `false`.
    let here = matches.try_contains_id(option).unwrap_or(false)
        && matches.value_source(option) == Some(ValueSource::CommandLine);
    here || matches
        .subcommand()
        .is_some_and(|(_, inner)| named_by_the_caller(inner, option))
}

/// The option's own spelling, as the parser renders it: `--json`, or `--root <PATH>` where
/// it takes a value.
///
/// Asked of the parser rather than written down a second time, so a renamed option or a
/// changed value placeholder reaches this sentence without anyone remembering to. An id
/// the parser has never heard of falls back to its bare long form, and a unit test below
/// refuses one: an exclusion naming an option that does not exist excludes nothing, and
/// the fallback would let it read as though it did.
fn spelled(command: &clap::Command, option: &str) -> String {
    declared(command, option).map_or_else(|| format!("--{option}"), |argument| argument.to_string())
}

/// The argument this id names, wherever in the command tree it is declared.
fn declared<'a>(command: &'a clap::Command, option: &str) -> Option<&'a clap::Arg> {
    command
        .get_arguments()
        .find(|argument| argument.get_id().as_str() == option)
        .or_else(|| {
            command
                .get_subcommands()
                .find_map(|family| declared(family, option))
        })
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
    // Before every refusal below, because these are about the options rather than about
    // any one command's assignments, and because that is where the parser decided them
    // for as long as they were declared on the options themselves.
    if let Some(message) = excluded_globals(&command, &matches) {
        return Err(command.error(ErrorKind::ArgumentConflict, message));
    }
    if let Family::Init { assignments, .. } = &cli.family
        && let Some(repeated) = repeated_assignment(assignments)
    {
        // One assignment set, one value per key. Letting the last one win would make a
        // command line that says two things do one of them silently, which is the same
        // refusal two output modes at once already gets.
        return Err(command.error(
            ErrorKind::ValueValidation,
            format!("`{}` is assigned twice", repeated.as_str()),
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
        // `ValueValidation` rather than `ArgumentConflict`, which this module reserves for
        // the exclusions it decides between options. See `GLOBAL_EXCLUSIONS`.
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
fn repeated_assignment(assignments: &[Assignment]) -> Option<ConfigurationPathKind> {
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
/// back to the parser's own first line, which is its terse statement of the problem —
/// including every refusal this module composes itself, whose whole message is that one
/// line. The rest of what the parser renders is usage and a pointer to `--help`, and that
/// belongs on standard error in human mode rather than inside a response document.
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

fn terse(error: &clap::Error) -> String {
    let rendered = error.render().to_string();
    let first = rendered.lines().next().unwrap_or_default();
    first
        .strip_prefix("error: ")
        .unwrap_or(first)
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Decision sources: Define the Rust CLI capability contract.

    fn arguments(rest: &[&str]) -> Vec<OsString> {
        std::iter::once(OsString::from("spielorders"))
            .chain(rest.iter().map(OsString::from))
            .collect()
    }

    #[test]
    fn the_default_target_is_the_working_directory() {
        let cli = parse(&arguments(&["package", "readme"])).expect("the command parses");
        assert_eq!(cli.options.root, PathBuf::from("."));
        assert_eq!(cli.options.mode(), Mode::Human);
    }

    #[test]
    fn each_output_mode_is_selected_on_its_own() {
        for (flag, mode) in [
            ("--json", Mode::Json),
            ("--quiet", Mode::Quiet),
            ("--verbose", Mode::Verbose),
        ] {
            let cli = parse(&arguments(&["package", "readme", flag])).expect("the mode parses");
            assert_eq!(cli.options.mode(), mode, "{flag}");
        }
    }

    /// An exclusion is a set of ids, and an id is tied to the option it names only by
    /// being spelled the same. A misspelled one excludes nothing and says nothing, because
    /// nothing was ever written on the command line under that name.
    #[test]
    fn every_excluded_option_is_one_the_parser_declares() {
        let command = Cli::command();
        for exclusion in GLOBAL_EXCLUSIONS {
            for option in *exclusion {
                assert!(
                    declared(&command, option).is_some(),
                    "`{option}` excludes another global option and is not one"
                );
            }
        }
    }

    #[test]
    fn two_output_modes_are_refused_rather_than_ordered() {
        for pair in [
            ["--json", "--quiet"],
            ["--json", "--verbose"],
            ["--quiet", "--verbose"],
        ] {
            // Together and split across the subcommand, because the sentence is composed
            // from the exclusion rather than from the two the parser happened to see, and
            // the second of these is where a declared conflict saw neither.
            for line in [
                vec!["package", "readme", pair[0], pair[1]],
                vec![pair[1], "package", "readme", pair[0]],
            ] {
                let error =
                    parse(&arguments(&line)).expect_err("two modes cannot both be selected");
                assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{line:?}");
                assert_eq!(
                    usage_message(&error),
                    format!("`{}` and `{}` are mutually exclusive", pair[0], pair[1]),
                    "{line:?}: the sentence names both modes in one stable order"
                );
            }
        }
    }

    #[test]
    fn an_unrecognized_option_is_named_in_the_usage_message() {
        let error =
            parse(&arguments(&["package", "readme", "--jsom"])).expect_err("the option is unknown");
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
        assert_eq!(usage_message(&error), "unrecognized option `--jsom`");
    }

    /// A usage message never carries the generated usage block or the pointer to
    /// `--help`: that text is for a human on standard error, and inside a response
    /// document it is the stream contamination the contract forbids.
    #[test]
    fn no_usage_message_carries_generated_help() {
        for rest in [
            vec!["package", "readme", "--jsom"],
            vec!["package", "readme", "--json", "--quiet"],
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
