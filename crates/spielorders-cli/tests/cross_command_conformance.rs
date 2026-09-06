//! Every command against one contract, checked as a set rather than one command at a time.
//!
//! **The inventory is the evidence.** [`DECIDED_COMMANDS`] transcribes the command tree the
//! decisions name — not the tree the executable happens to have — and it is read in both
//! directions: a decided command the executable lacks fails here, and so does a registered
//! contract no decided command claims. A family another milestone owns is recorded with the
//! milestone that owns it and asserted *absent*, so a `search` shipped early is a failure
//! rather than a surprise.
//!
//! **Every claim below is made over the whole set.** The tables are exhaustive matches on
//! `ResponseContract`, so a command added later is not merely encouraged to conform — it
//! does not compile until somebody says how it is reached, what cells it supplies, and
//! whether it mutates, and then it is held to every rule here without anyone remembering to
//! write a test for it. A check that examined a hand-written list would let the next command
//! opt out by being left off it.
//!
//! What is deliberately *not* here is any command's own bytes: `tests/response_contract.rs`
//! compares each recorded cell, and this file says which cells there should be.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders agent
//! skill contract.

use crate::support::{
    ENVELOPE, Invocation, NEWEST_ORDER_ID, TemporaryTree, directory_that_is_not_a_root,
    entry_names, fixture_directory, managed_snapshot, order_directory,
    package_with_an_adoption_candidate, package_with_three_contacts, populated_package, response,
    unmanaged_directory,
};
use spielorders_cli::{cli::GLOBAL_EXCLUSIONS, contract::ResponseContract};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The inventory, built from the decisions rather than from the code.
// ---------------------------------------------------------------------------

/// What this repository does about one decided command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Disposition {
    /// Delivered here, under its own independently versioned response contract.
    Own(ResponseContract),
    /// Delivered here, rendering another command's contract for a human. A command line
    /// that shares one records no cells of its own — the sharing is pinned behaviourally,
    /// and the contract's own cells are the record.
    Shared(ResponseContract),
    /// Another milestone's, named. M4 delivers no part of it, and the check below asserts
    /// the executable does not answer it: shipping one early would expose a surface whose
    /// engine does not exist yet.
    Routed(&'static str),
}

/// Every command the decisions name, with the command line a caller writes.
///
/// **Transcribed from Define the Rust CLI capability contract's command surface**, in its
/// order, plus `adopt-order` — which that tree omits and Define legacy Order adoption and
/// metadata bootstrapping requires — and the two `agent` commands Define the SpielOrders
/// agent skill contract names. Each `package values <kind>` is counted separately, because a
/// parameter is a segment of the identity like any other.
const DECIDED_COMMANDS: &[(&[&str], Disposition)] = &[
    (&["init"], Disposition::Own(ResponseContract::Init)),
    (&["add-order"], Disposition::Own(ResponseContract::AddOrder)),
    (
        &["add-vendor"],
        Disposition::Own(ResponseContract::AddVendor),
    ),
    (
        &["adopt-order"],
        Disposition::Own(ResponseContract::AdoptOrder),
    ),
    (
        &["package", "show"],
        Disposition::Own(ResponseContract::PackageShow),
    ),
    (
        &["package", "readme"],
        Disposition::Own(ResponseContract::PackageReadme),
    ),
    (
        &["package", "list", "orders"],
        Disposition::Own(ResponseContract::PackageListOrders),
    ),
    (
        &["package", "list", "vendors"],
        Disposition::Own(ResponseContract::PackageListVendors),
    ),
    (
        &["package", "values", "project"],
        Disposition::Own(ResponseContract::PackageValuesProject),
    ),
    (
        &["package", "values", "orderer"],
        Disposition::Own(ResponseContract::PackageValuesOrderer),
    ),
    (
        &["package", "values", "ordering_institution"],
        Disposition::Own(ResponseContract::PackageValuesOrderingInstitution),
    ),
    (
        &["package", "values", "order_group"],
        Disposition::Own(ResponseContract::PackageValuesOrderGroup),
    ),
    (
        &["order", "show"],
        Disposition::Own(ResponseContract::OrderShow),
    ),
    (
        &["order", "edit"],
        Disposition::Own(ResponseContract::OrderEdit),
    ),
    (
        &["order", "readme"],
        Disposition::Own(ResponseContract::OrderReadme),
    ),
    (
        &["order", "align"],
        Disposition::Own(ResponseContract::OrderAlign),
    ),
    (
        &["order", "remove"],
        Disposition::Own(ResponseContract::OrderRemove),
    ),
    (
        &["order", "validate"],
        Disposition::Own(ResponseContract::OrderValidate),
    ),
    (
        &["order", "repair"],
        Disposition::Own(ResponseContract::OrderRepair),
    ),
    (
        &["order", "item", "list"],
        Disposition::Shared(ResponseContract::OrderShow),
    ),
    (
        &["order", "item", "add"],
        Disposition::Own(ResponseContract::OrderItemAdd),
    ),
    (
        &["order", "item", "edit"],
        Disposition::Own(ResponseContract::OrderItemEdit),
    ),
    (
        &["order", "item", "move"],
        Disposition::Own(ResponseContract::OrderItemMove),
    ),
    (
        &["order", "item", "remove"],
        Disposition::Own(ResponseContract::OrderItemRemove),
    ),
    (
        &["vendor", "show"],
        Disposition::Own(ResponseContract::VendorShow),
    ),
    (
        &["vendor", "edit"],
        Disposition::Own(ResponseContract::VendorEdit),
    ),
    (
        &["vendor", "hide"],
        Disposition::Own(ResponseContract::VendorHide),
    ),
    (
        &["vendor", "unhide"],
        Disposition::Own(ResponseContract::VendorUnhide),
    ),
    (
        &["vendor", "contact", "list"],
        Disposition::Shared(ResponseContract::VendorShow),
    ),
    (
        &["vendor", "contact", "add"],
        Disposition::Own(ResponseContract::VendorContactAdd),
    ),
    (
        &["vendor", "contact", "edit"],
        Disposition::Own(ResponseContract::VendorContactEdit),
    ),
    (
        &["vendor", "contact", "move"],
        Disposition::Own(ResponseContract::VendorContactMove),
    ),
    (
        &["vendor", "contact", "remove"],
        Disposition::Own(ResponseContract::VendorContactRemove),
    ),
    (&["search", "orders"], Disposition::Routed("M5")),
    (&["search", "vendors"], Disposition::Routed("M5")),
    (
        &["search", "supporting-material"],
        Disposition::Routed("M5"),
    ),
    (&["validate"], Disposition::Own(ResponseContract::Validate)),
    (&["repair"], Disposition::Own(ResponseContract::Repair)),
    (&["upgrade"], Disposition::Own(ResponseContract::Upgrade)),
    (
        &["index", "status"],
        Disposition::Own(ResponseContract::IndexStatus),
    ),
    (
        &["index", "rebuild"],
        Disposition::Own(ResponseContract::IndexRebuild),
    ),
    (&["agent", "prepare"], Disposition::Routed("M8")),
    (&["agent", "status"], Disposition::Routed("M8")),
];

/// Every decided command this milestone delivers is a command line the executable answers,
/// and every family it routes is one the executable does not.
///
/// **The forward direction of the inventory.** Generated `--help` is what asks the
/// question: a command line the parser recognizes renders its reference and exits `0`, and
/// one it does not is a usage error at exit `2`. That is also the whole of what M8's
/// generated references cite, so it is confirmed here for every command rather than
/// sampled.
#[test]
fn every_decided_command_is_delivered_or_routed_to_the_milestone_that_owns_it() {
    for (line, disposition) in DECIDED_COMMANDS {
        let mut run = Invocation::new(line);
        run = run.argument("--help");
        let outcome = run.run();

        match disposition {
            Disposition::Own(_) | Disposition::Shared(_) => {
                assert_eq!(
                    outcome.status,
                    Some(0),
                    "{line:?} is decided and this executable does not answer it: {}",
                    outcome.stderr_text()
                );
                assert_eq!(outcome.stderr, b"", "{line:?}: the reference is an answer");
                let text = outcome.stdout_text();
                assert!(text.contains("Usage:"), "{line:?}: {text}");
                let leaf = line.last().expect("a command line has a last segment");
                assert!(
                    text.contains(leaf),
                    "{line:?}: the reference names the command it is about: {text}"
                );
            }
            Disposition::Routed(milestone) => {
                assert_eq!(
                    outcome.status,
                    Some(2),
                    "{line:?} is {milestone}'s and this milestone answers it: {}",
                    outcome.stdout_text()
                );
            }
        }
    }
}

/// Every registered contract is claimed by exactly one decided command.
///
/// **The reverse direction of the inventory**, which is the one nothing did before: a
/// contract the executable registers and no decision names would ship a command surface
/// nobody decided. `spielorders.usage` is the exception and says why — a usage error is not
/// a command's response, so no command claims it.
#[test]
fn every_registered_contract_is_claimed_by_a_decided_command() {
    for contract in ResponseContract::REGISTERED {
        if *contract == ResponseContract::Usage {
            continue;
        }
        let claims: Vec<&[&str]> = DECIDED_COMMANDS
            .iter()
            .filter(|(_, disposition)| *disposition == Disposition::Own(*contract))
            .map(|(line, _)| *line)
            .collect();
        assert_eq!(
            claims.len(),
            1,
            "{} is claimed by {claims:?}, and a contract names exactly one command shape",
            contract.name()
        );
    }
}

/// The one identity no command claims, because no command produced it.
#[test]
fn the_usage_contract_belongs_to_no_command() {
    assert!(
        !DECIDED_COMMANDS.iter().any(|(_, disposition)| matches!(
            disposition,
            Disposition::Own(ResponseContract::Usage)
                | Disposition::Shared(ResponseContract::Usage)
        )),
        "a usage error is not a command's response: parsing failed, so no command ran"
    );
    assert!(
        ResponseContract::REGISTERED.contains(&ResponseContract::Usage),
        "and it is registered all the same, because every usage error carries it"
    );
}

/// No contract has advanced, and none may until the release the CLI capability decision
/// names.
///
/// **The rule most likely to be broken by habit.** Until the owner declares the CLI and the
/// agent support tooling released — which cannot happen before M4 and M8 both close — every
/// interface, format, and protocol is revised in place and every `contract_version` is 1.
/// A change to a payload edits version 1 and re-records that contract's cells; a second
/// version of a contract nobody has read makes the first read as history when it is still a
/// draft.
#[test]
fn no_contract_has_advanced_past_its_first_version() {
    for contract in ResponseContract::REGISTERED {
        assert_eq!(
            contract.version(),
            1,
            "{} advanced, and nothing has run outside a test tree to be broken by a change",
            contract.name()
        );
    }
}

// ---------------------------------------------------------------------------
// The golden-fixture matrix, over every contract at once.
// ---------------------------------------------------------------------------

/// Every response kind a contract's fixture directory can hold a cell for.
///
/// `usage-error` is not among them and never will be: it is not any command's cell, it
/// carries the one shared identity, and [`the_usage_contract_supplies_only_its_own_cell`]
/// is where that is held.
const CELLS: [&str; 5] = ["success", "empty-result", "warning", "invalid", "rejected"];

/// Whether a contract supplies one cell, or the reason it cannot reach it.
///
/// **An absent fixture and an impossible one must not look alike**, which is the whole
/// point of writing the reasons down: a hole with a reason is a decision, and a hole
/// without one is an omission nobody has noticed yet.
#[derive(Clone, Copy, Debug)]
enum Supply {
    Recorded,
    Unreachable(&'static str),
}

/// What every registered contract supplies, and why it supplies nothing else.
///
/// **The one home for the completeness of the matrix.** An exhaustive match, so a contract
/// registered later does not compile until somebody has said, cell by cell, what it records
/// and what it cannot reach — which is the question that is easiest to leave unanswered and
/// costs the most when it is.
///
/// Three reasons recur, and are given once here rather than restated per contract:
///
/// - **`empty-result` is unreachable for every mutation.** An answer that there is nothing
///   there is not one a mutation gives: each reports the record it created, changed, or
///   moved, and a refusal to touch one is `rejected`.
/// - **`rejected` is unreachable for every read.** It is a mutation's answer, and a read has
///   no mutation to refuse. A subject that does not resolve is `invalid` for a read and
///   `rejected` for a mutation, because a cell names the kind of response and not the
///   condition.
/// - **A dry run has no cell of its own** and needs none: it is the `success` document with
///   `committed: false`, pinned against the real run of the same plan by each command's own
///   suite.
fn matrix(contract: ResponseContract) -> [(&'static str, Supply); 5] {
    const READ: [(&str, Supply); 5] = [
        ("success", Supply::Recorded),
        ("empty-result", Supply::Recorded),
        ("warning", Supply::Recorded),
        ("invalid", Supply::Recorded),
        (
            "rejected",
            Supply::Unreachable("a read refuses no mutation"),
        ),
    ];
    const MUTATION: [(&str, Supply); 5] = [
        ("success", Supply::Recorded),
        (
            "empty-result",
            Supply::Unreachable(
                "a mutation reports what it changed, so there is always \
                                 something to report",
            ),
        ),
        ("warning", Supply::Recorded),
        ("invalid", Supply::Recorded),
        ("rejected", Supply::Recorded),
    ];
    match contract {
        // A usage error is not a command's response. Its one cell is the shared
        // `usage-error`, which is not one of the five and lives outside this table.
        ResponseContract::Usage => CELLS.map(|cell| {
            (
                cell,
                Supply::Unreachable("a usage error is the shared identity's own cell"),
            )
        }),
        // The reads: every collection is present and possibly empty, a README or a record
        // that could not be read is a warning beside the answer, and a subject the load
        // could read and not trust is a failed response carrying the diagnosis.
        ResponseContract::PackageReadme
        | ResponseContract::PackageShow
        | ResponseContract::PackageListOrders
        | ResponseContract::PackageListVendors
        | ResponseContract::PackageValuesOrderingInstitution
        | ResponseContract::PackageValuesOrderer
        | ResponseContract::PackageValuesProject
        | ResponseContract::PackageValuesOrderGroup
        | ResponseContract::OrderShow
        | ResponseContract::OrderReadme
        | ResponseContract::VendorShow
        | ResponseContract::Validate => READ,
        // The mutations of a record that exists, of a collection embedded in one, and of a
        // whole Order Directory.
        ResponseContract::OrderEdit
        | ResponseContract::OrderItemAdd
        | ResponseContract::OrderItemEdit
        | ResponseContract::OrderItemMove
        | ResponseContract::OrderItemRemove
        | ResponseContract::OrderAlign
        | ResponseContract::OrderRemove
        | ResponseContract::AddOrder
        | ResponseContract::AdoptOrder
        | ResponseContract::AddVendor
        | ResponseContract::VendorEdit
        | ResponseContract::VendorHide
        | ResponseContract::VendorUnhide
        | ResponseContract::VendorContactAdd
        | ResponseContract::VendorContactEdit
        | ResponseContract::VendorContactMove
        | ResponseContract::VendorContactRemove => MUTATION,
        // `init`'s subject is a directory that must *not* be a Package, so it reads none
        // and has no untrusted subject to report; and the core defines no initialization
        // warning at all, which `InitializationWarning`'s having no variants is what says.
        ResponseContract::Init => [
            ("success", Supply::Recorded),
            (
                "empty-result",
                Supply::Unreachable("init answers with the Package it just created"),
            ),
            (
                "warning",
                Supply::Unreachable(
                    "InitializationWarning is uninhabited, and an initialization assesses \
                     no Package, so it observes nothing either",
                ),
            ),
            (
                "invalid",
                Supply::Unreachable(
                    "init reads no Package, so it has no subject it could read and not \
                     trust; every failure it has is a refused mutation",
                ),
            ),
            ("rejected", Supply::Recorded),
        ],
        // A scoped validation's payload is one member naming the scope the findings are
        // about, and a member that is always present is never an empty answer. The findings
        // are the envelope's, and none over a healthy Order is `success`.
        ResponseContract::OrderValidate => [
            ("success", Supply::Recorded),
            (
                "empty-result",
                Supply::Unreachable("the scope this run examined is always named"),
            ),
            ("warning", Supply::Recorded),
            ("invalid", Supply::Recorded),
            (
                "rejected",
                Supply::Unreachable("a validation refuses no mutation"),
            ),
        ],
        // Both moods of one command, so a diagnosis with nothing actionable is the empty
        // answer and a refused application is the rejected one.
        ResponseContract::Repair | ResponseContract::OrderRepair => {
            CELLS.map(|cell| (cell, Supply::Recorded))
        }
        // An upgrade always answers with a source, a target and the artifacts it would
        // visit; and this build's registered transformation chain is empty, so every
        // Package it can read is already at the target and a commit that carried something
        // — which a warning needs — cannot happen.
        ResponseContract::Upgrade => [
            ("success", Supply::Recorded),
            (
                "empty-result",
                Supply::Unreachable("an upgrade always names its source and its target"),
            ),
            (
                "warning",
                Supply::Unreachable(
                    "TransformationRegistry::registered() is empty, so a commit answers \
                     Unchanged before it can carry anything to warn about",
                ),
            ),
            ("invalid", Supply::Recorded),
            ("rejected", Supply::Recorded),
        ],
        // The index has a state in every case, and `missing` is one of the states this
        // command reports rather than an absent answer.
        ResponseContract::IndexStatus => [
            ("success", Supply::Recorded),
            (
                "empty-result",
                Supply::Unreachable("the index is in one of its states in every case"),
            ),
            ("warning", Supply::Recorded),
            ("invalid", Supply::Recorded),
            (
                "rejected",
                Supply::Unreachable("index status writes nothing at all"),
            ),
        ],
        // The warning every other mutation carries is that its canonical write succeeded
        // and the derived refresh did not — and the derived index is this command's whole
        // subject, so a rebuild that could not replace it wrote nothing and is `rejected`.
        // `PackageIndexRebuildWarning` is uninhabited, which is the core saying so.
        ResponseContract::IndexRebuild => [
            ("success", Supply::Recorded),
            (
                "empty-result",
                Supply::Unreachable("a mutation reports what it changed"),
            ),
            (
                "warning",
                Supply::Unreachable(
                    "PackageIndexRebuildWarning is uninhabited: this command's whole \
                     subject is the derived index, so a refresh that failed is the refusal",
                ),
            ),
            ("invalid", Supply::Recorded),
            ("rejected", Supply::Recorded),
        ],
    }
}

/// Every contract's fixture directory holds exactly the cells the matrix says it supplies.
///
/// **One check over the whole set**, so a contract whose fixtures nobody added, a cell
/// recorded under a name the matrix does not know, and a hole nobody accounted for are all
/// the same failure — and none of them depends on somebody having written a test for that
/// command.
#[test]
fn every_contract_records_exactly_the_cells_the_matrix_supplies() {
    for contract in ResponseContract::REGISTERED {
        if *contract == ResponseContract::Usage {
            continue;
        }
        let mut expected: Vec<String> = matrix(*contract)
            .into_iter()
            .filter(|(_, supply)| matches!(supply, Supply::Recorded))
            .map(|(cell, _)| format!("{cell}.json"))
            .collect();
        expected.sort();
        let directory = fixture_directory(*contract);
        assert_eq!(
            entry_names(&directory),
            expected,
            "{} records a different set of cells from the one the matrix supplies, in {}",
            contract.name(),
            directory.display()
        );
    }
}

/// Every hole in the matrix carries a reason, and every reason says something.
///
/// A hole with an empty reason is the omission this table exists to prevent, dressed as a
/// decision.
#[test]
fn every_unreachable_cell_carries_the_reason_it_is_unreachable() {
    for contract in ResponseContract::REGISTERED {
        let cells = matrix(*contract);
        assert_eq!(
            cells.map(|(cell, _)| cell),
            CELLS,
            "{} names the five response kinds, in one order",
            contract.name()
        );
        for (cell, supply) in cells {
            if let Supply::Unreachable(reason) = supply {
                assert!(
                    reason.len() > 20,
                    "{} says nothing about why it cannot reach `{cell}`",
                    contract.name()
                );
            }
        }
    }
}

/// The usage contract's directory holds its one cell and no other.
#[test]
fn the_usage_contract_supplies_only_its_own_cell() {
    assert_eq!(
        entry_names(&fixture_directory(ResponseContract::Usage)),
        vec!["usage-error.json".to_owned()],
        "a usage error is one response, shared by every command line that reaches exit `2`"
    );
}

/// The placeholder a recorded response carries where the run's own Package Root stood.
///
/// A fixture records the Root as this rather than as whatever temporary directory the run
/// used, so a recorded path is rooted at the placeholder exactly where a live one is rooted
/// at an absolute path. The substitution asserts the shape of what it replaces and every
/// other byte is compared exactly; see [the fixture convention].
///
/// [the fixture convention]: ../../../../docs/development/traceability.md
const RECORDED_PACKAGE_ROOT: &str = "<PACKAGE_ROOT>";

/// Every path a command reports is rooted, and carries the Package's own spelling for it
/// wherever the same response names the Package it is inside — and carries none where the
/// response names no Package at all.
///
/// **The claim a type could not keep on its own.** The location projections make the pair
/// structural, and refusal cells across eight contracts still recorded a resolved path
/// inside an established Package with no Package-relative spelling — because the value
/// handed to the projection had thrown the spelling away before it got there, which no
/// amount of structure downstream can recover. So the agreement is checked over every cell
/// every contract records, rather than over the runs somebody thought to write a test for,
/// and a cell recorded later is checked by having been recorded.
///
/// **Both directions, because the forward one alone is switched off by exactly what it
/// exists to catch.** A cell whose `context` names no Root skips the check that would
/// notice it spelling a path the Package's way, so the one response that breaks the
/// convention is the one response the check declines to look at. A relative spelling is
/// proof a Package was resolved, so a response carrying one and naming no Root contradicts
/// itself, and that is asked here rather than left to the prose below.
///
/// The cells are the whole set precisely because
/// [`every_contract_records_exactly_the_cells_the_matrix_supplies`] says so, and
/// `tests/response_contract.rs` is what ties each of them to the bytes a real process wrote.
///
/// `init`'s refusals are the one hole in the forward direction, and it is the hole
/// [`no_command_finds_its_root_by_searching_an_ancestor`] leaves for the same reason: its
/// subject is a directory that must *not* be a Package, so a refused initialization names a
/// directory no Package ever stood in and a path there has no Package to be relative to.
/// The converse still holds over them: no Package stood there, so nothing may be spelled
/// relative to one.
///
/// A cell is one refusal, so a condition no contract records is not checked here at all.
/// [`every_refusal_an_interrupted_write_reaches_carries_the_packages_own_spelling_for_it`]
/// is where the same claim is made over responses instead.
#[test]
fn every_recorded_path_carries_the_packages_own_spelling_for_it() {
    let mut located = false;
    let mut failed = false;
    let mut rootless = false;
    for (contract, label, document) in recorded_cells() {
        let mut reported = Vec::new();
        collect_reported_paths(&document, &mut reported);
        for (shape, path, _) in &reported {
            match *shape {
                "absolute_path" => located = true,
                _ => failed = true,
            }
            assert!(
                path.starts_with(RECORDED_PACKAGE_ROOT),
                "{label}: `{path}` is reported without the Root it is absolute from"
            );
        }
        // A refusal `init` records names the directory it was pointed at, and no Package
        // stands there to spell anything relative to — as does any cell whose response
        // resolved no Root, whatever the fixture's text substitution makes its paths
        // look like.
        let names_a_package = document["context"]["package_root"].as_str()
            == Some(RECORDED_PACKAGE_ROOT)
            && !(contract == ResponseContract::Init && document["ok"] == false);
        if names_a_package {
            assert_spelled_by_the_package_at(&reported, RECORDED_PACKAGE_ROOT, &label);
            continue;
        }
        if document["context"]["package_root"].is_null() {
            rootless = true;
        }
        for (_, path, relative) in &reported {
            assert_eq!(
                *relative, None,
                "{label}: `{path}` is spelled the way a Package spells it while this \
                 response names no Package to spell it against"
            );
        }
    }
    // Both shapes seen rather than a count of paths, because a count is a number a reader
    // has to trust — and a walk that quietly matched neither shape would pass over
    // everything.
    assert!(
        located,
        "a location inside a Package is one of the two shapes"
    );
    assert!(failed, "a failure's own path is the other");
    // The converse is asked of a real cell rather than of none: a suite where every cell
    // named a Root would make it vacuous exactly as the forward direction was.
    assert!(rootless, "some cell answers about no Package at all");
}

/// The claim itself, over the paths one response reported and the Root it was about.
///
/// **One home for the rule**, because it is now asked twice: once of every recorded cell,
/// and once of every response an interrupted write refuses. Both halves are asked, because
/// both are the rule: a path inside `root` owes the spelling that Root has for it, in a
/// refusal exactly as in a success, and a path outside it has no Package to be relative to
/// and owes `null`.
fn assert_spelled_by_the_package_at(
    reported: &[(&str, &str, Option<&str>)],
    root: &str,
    label: &str,
) {
    for (_, path, relative) in reported {
        let expected = match path.strip_prefix(root) {
            Some("") => Some("."),
            // A Root that is a text prefix of some other path is not a Root above it, so
            // `strip_prefix` answering `None` here is the outside case too.
            Some(rest) => rest.strip_prefix('/'),
            None => None,
        };
        assert_eq!(
            *relative, expected,
            "{label}: `{path}` and the Package Root this response names disagree \
             about how this Package spells it"
        );
    }
}

/// Every domain identity a command reports is a UUIDv4, or the `null` that says it has
/// none.
///
/// **An identity is minted by the core and reported by every command that has one**, so
/// "domain UUIDs" is a claim about the whole set rather than about whichever command a
/// reader checks. What it rules out is a locator, a Package-relative name, or a planning
/// placeholder standing where a client expects an identity it can store — the last of which
/// a run asked only to plan has, and answers with `null` instead.
///
/// A recorded identity a run minted is one of the closed placeholder set, and each of those
/// substitutions asserts a lowercase hyphenated UUIDv4 before replacing it — so a cell
/// carrying one has had this checked for it by the comparison that recorded it.
#[test]
fn every_domain_identity_a_command_reports_is_a_uuid() {
    let mut checked = false;
    for (_, label, document) in recorded_cells() {
        let mut reported = Vec::new();
        collect_reported_identities(&document, &mut reported);
        for (member, identity) in reported {
            checked = true;
            let Some(value) = identity else {
                continue;
            };
            if RECORDED_MINTED_IDENTITIES.contains(&value) {
                continue;
            }
            assert!(
                is_uuid(value),
                "{label}: `{member}` is `{value}`, which is not a domain identity"
            );
        }
    }
    assert!(
        checked,
        "the recorded cells report identities, and the walk found them"
    );
}

/// The placeholders a recorded response carries where a run minted an identity.
///
/// Two of the closed placeholder set the fixture convention names; the other two stand for
/// a Package Root and for a date read off the clock, neither of which is an identity.
const RECORDED_MINTED_IDENTITIES: [&str; 2] = ["<VENDOR_ID>", "<ORDER_ID>"];

fn is_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups.iter().map(|group| group.len()).eq([8, 4, 4, 4, 12])
        && value
            .chars()
            .filter(|character| *character != '-')
            .all(|character| character.is_ascii_digit() || character.is_ascii_lowercase())
        && groups[2].starts_with('4')
}

/// Every domain identity a response reports, by the member that names it.
///
/// The member's name is what says it is one: an identity travels under the record's own
/// field name, which is how the core spells it, and nothing else in a response is named
/// that way.
fn collect_reported_identities<'a>(
    value: &'a serde_json::Value,
    found: &mut Vec<(&'a str, Option<&'a str>)>,
) {
    match value {
        serde_json::Value::Object(members) => {
            for (member, held) in members {
                if member.ends_with("_id") {
                    found.push((member, held.as_str()));
                }
                collect_reported_identities(held, found);
            }
        }
        serde_json::Value::Array(members) => {
            for member in members {
                collect_reported_identities(member, found);
            }
        }
        _ => {}
    }
}

/// Every cell every registered contract records, parsed, with the label a failure names it
/// by.
///
/// The fixture tree is the record of what the commands wrote —
/// `tests/response_contract.rs` is what ties each cell to the bytes a real process
/// produced — so a claim about every command's response is made here over every cell rather
/// than over the runs somebody thought to write a test for.
fn recorded_cells() -> Vec<(ResponseContract, String, serde_json::Value)> {
    let mut cells = Vec::new();
    for contract in ResponseContract::REGISTERED {
        let directory = fixture_directory(*contract);
        for cell in entry_names(&directory) {
            let label = format!("{} {cell}", contract.name());
            let recorded = std::fs::read_to_string(directory.join(&cell))
                .unwrap_or_else(|error| panic!("{label}: {error}"));
            let document = serde_json::from_str(&recorded)
                .unwrap_or_else(|error| panic!("{label}: a recorded cell is a response: {error}"));
            cells.push((*contract, label, document));
        }
    }
    cells
}

/// Every path a response reports, as (the member naming it, the path, its Package-relative
/// spelling).
///
/// **Two shapes and no third.** A location inside a Package carries `absolute_path`; a
/// failure carries `path` beside a `spelling`, because resolving a caller-supplied path is
/// the one operation that can fail before there is anything absolute to name. A third shape
/// fails here rather than going unchecked: the Package-relative spelling is what identifies
/// a reported path, and one recorded without a path beside it is a shape nobody decided.
fn collect_reported_paths<'a>(
    value: &'a serde_json::Value,
    found: &mut Vec<(&'a str, &'a str, Option<&'a str>)>,
) {
    match value {
        serde_json::Value::Object(members) => {
            if members.contains_key("package_relative_path") {
                let (shape, path) = ["absolute_path", "path"]
                    .into_iter()
                    .find_map(|shape| Some((shape, members.get(shape)?.as_str()?)))
                    .expect("a Package-relative spelling is recorded beside the path it spells");
                found.push((shape, path, members["package_relative_path"].as_str()));
            }
            for member in members.values() {
                collect_reported_paths(member, found);
            }
        }
        serde_json::Value::Array(members) => {
            for member in members {
                collect_reported_paths(member, found);
            }
        }
        _ => {}
    }
}

/// Nothing stands under `contracts/cli/` that no registered contract claims.
///
/// **The walk in the reverse direction**, which nothing did before: every check over the
/// fixtures started from the registry and asked whether the files were there, so a
/// directory left behind by a renamed contract — or one added for a command that was never
/// registered — was invisible. A fixture nothing reads is worse than no fixture: it looks
/// like evidence.
#[test]
fn no_fixture_directory_is_unclaimed_by_a_registered_contract() {
    let root = contracts_cli_root();
    let mut claimed: Vec<PathBuf> = ResponseContract::REGISTERED
        .iter()
        .map(|contract| root.join(contract.fixture_directory()))
        .collect();
    claimed.sort();

    let mut found = Vec::new();
    collect_fixture_directories(&root, &mut found);
    found.sort();

    assert_eq!(
        found
            .iter()
            .map(|path| relative_to(&root, path))
            .collect::<Vec<_>>(),
        claimed
            .iter()
            .map(|path| relative_to(&root, path))
            .collect::<Vec<_>>(),
        "every directory holding response fixtures is one a registered contract names, and \
         every registered contract has one"
    );
}

/// The one canonical contract root, resolved the way every consumer resolves it.
fn contracts_cli_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/cli")
        .canonicalize()
        .expect("the CLI contract root stands where the convention says")
}

/// Every directory under `contracts/cli` that holds a file, which is what a fixture
/// directory is.
fn collect_fixture_directories(directory: &std::path::Path, found: &mut Vec<PathBuf>) {
    let mut holds_a_file = false;
    for name in entry_names(directory) {
        let path = directory.join(name);
        if path.is_dir() {
            collect_fixture_directories(&path, found);
        } else {
            holds_a_file = true;
        }
    }
    if holds_a_file {
        found.push(directory.to_path_buf());
    }
}

fn relative_to(root: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

// ---------------------------------------------------------------------------
// How every contract is reached, and what it owes when it is.
// ---------------------------------------------------------------------------

/// What `--root` names for one command, and what has to stand there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Subject {
    /// The SpielOrders Root of a Package with Orders, Vendors, Contacts and Items in it.
    PackageRoot,
    /// The same Root, with three Contacts on the Vendor it offers. A `move` within a
    /// collection of one is the same position twice, which the core answers as an
    /// unchanged mutation without replacing anything — so the one command that moves a
    /// Contact needs somewhere to move it before its commit is a commit at all.
    PackageRootWithThreeContacts,
    /// One Order Directory of that Package. An `order` command's `--root` names the Order
    /// Directory and never the Root above it.
    OrderRoot,
    /// A directory that is not a Package, which is the only subject `init` accepts.
    UnmanagedDirectory,
    /// A Package Root with one unmanaged candidate under its Orders Directory, which the
    /// command line names positionally.
    AdoptionCandidate,
    /// No command line reaches it.
    NoCommand,
}

/// One command line that reaches one contract, and what it needs to reach it.
struct Shape {
    /// The command line as a caller writes it, without options.
    line: &'static [&'static str],
    subject: Subject,
    /// The positionals and canonical assignments that make it a real run over
    /// [`populated_package`]'s Package.
    arguments: &'static [&'static str],
    /// Whether it takes `--dry-run`, which is every mutating command and no read.
    mutating: bool,
}

/// How each registered contract is reached, exhaustively.
///
/// **A match rather than a table**, so a contract registered later is a compile error until
/// somebody says how it is run — and is then held to every claim in this file by
/// construction. The arguments are the smallest command line that reaches a *successful*
/// answer over the shared fixture Package, because that is the run every claim below is
/// about; a refusal's own bytes are `tests/response_contract.rs`'s.
fn shape(contract: ResponseContract) -> Shape {
    let read = |line: &'static [&'static str], subject| Shape {
        line,
        subject,
        arguments: &[],
        mutating: false,
    };
    match contract {
        ResponseContract::Usage => Shape {
            line: &[],
            subject: Subject::NoCommand,
            arguments: &[],
            mutating: false,
        },
        ResponseContract::Init => Shape {
            line: &["init"],
            subject: Subject::UnmanagedDirectory,
            arguments: &[],
            mutating: true,
        },
        ResponseContract::PackageReadme => read(&["package", "readme"], Subject::PackageRoot),
        ResponseContract::PackageShow => read(&["package", "show"], Subject::PackageRoot),
        ResponseContract::PackageListOrders => {
            read(&["package", "list", "orders"], Subject::PackageRoot)
        }
        ResponseContract::PackageListVendors => {
            read(&["package", "list", "vendors"], Subject::PackageRoot)
        }
        ResponseContract::PackageValuesOrderingInstitution => read(
            &["package", "values", "ordering_institution"],
            Subject::PackageRoot,
        ),
        ResponseContract::PackageValuesOrderer => {
            read(&["package", "values", "orderer"], Subject::PackageRoot)
        }
        ResponseContract::PackageValuesProject => {
            read(&["package", "values", "project"], Subject::PackageRoot)
        }
        ResponseContract::PackageValuesOrderGroup => {
            read(&["package", "values", "order_group"], Subject::PackageRoot)
        }
        ResponseContract::OrderShow => read(&["order", "show"], Subject::OrderRoot),
        ResponseContract::OrderReadme => read(&["order", "readme"], Subject::OrderRoot),
        ResponseContract::OrderValidate => read(&["order", "validate"], Subject::OrderRoot),
        ResponseContract::VendorShow => Shape {
            line: &["vendor", "show"],
            subject: Subject::PackageRoot,
            arguments: &["Tarnwick"],
            mutating: false,
        },
        ResponseContract::Validate => read(&["validate"], Subject::PackageRoot),
        ResponseContract::IndexStatus => read(&["index", "status"], Subject::PackageRoot),
        ResponseContract::OrderEdit => Shape {
            line: &["order", "edit"],
            subject: Subject::OrderRoot,
            arguments: &["title=Laser heads"],
            mutating: true,
        },
        ResponseContract::OrderItemAdd => Shape {
            line: &["order", "item", "add"],
            subject: Subject::OrderRoot,
            arguments: &[
                "description=Fiber patch cable",
                "quantity=3",
                "unit_price=12.5",
            ],
            mutating: true,
        },
        ResponseContract::OrderItemEdit => Shape {
            line: &["order", "item", "edit"],
            subject: Subject::OrderRoot,
            arguments: &["1", "quantity=5"],
            mutating: true,
        },
        ResponseContract::OrderItemMove => Shape {
            line: &["order", "item", "move"],
            subject: Subject::OrderRoot,
            arguments: &["1", "position=2"],
            mutating: true,
        },
        ResponseContract::OrderItemRemove => Shape {
            line: &["order", "item", "remove"],
            subject: Subject::OrderRoot,
            arguments: &["1"],
            mutating: true,
        },
        ResponseContract::OrderAlign => Shape {
            line: &["order", "align"],
            subject: Subject::OrderRoot,
            arguments: &[],
            mutating: true,
        },
        ResponseContract::OrderRemove => Shape {
            line: &["order", "remove"],
            subject: Subject::OrderRoot,
            arguments: &[],
            mutating: true,
        },
        ResponseContract::OrderRepair => Shape {
            line: &["order", "repair"],
            subject: Subject::OrderRoot,
            arguments: &[],
            mutating: true,
        },
        ResponseContract::AddOrder => Shape {
            line: &["add-order"],
            subject: Subject::PackageRoot,
            arguments: &["title=Laser heads"],
            mutating: true,
        },
        ResponseContract::AdoptOrder => Shape {
            line: &["adopt-order"],
            subject: Subject::AdoptionCandidate,
            arguments: &[
                "title=Fiber patch cable",
                "date_ordered=2009-04-30",
                "status=complete",
            ],
            mutating: true,
        },
        ResponseContract::AddVendor => Shape {
            line: &["add-vendor"],
            subject: Subject::PackageRoot,
            arguments: &["name=Copperwick"],
            mutating: true,
        },
        ResponseContract::VendorEdit => Shape {
            line: &["vendor", "edit"],
            subject: Subject::PackageRoot,
            arguments: &["Tarnwick", "phone=+1 555 0100"],
            mutating: true,
        },
        ResponseContract::VendorHide => Shape {
            line: &["vendor", "hide"],
            subject: Subject::PackageRoot,
            arguments: &["Tarnwick"],
            mutating: true,
        },
        ResponseContract::VendorUnhide => Shape {
            line: &["vendor", "unhide"],
            subject: Subject::PackageRoot,
            arguments: &["Nethermoor"],
            mutating: true,
        },
        ResponseContract::VendorContactAdd => Shape {
            line: &["vendor", "contact", "add"],
            subject: Subject::PackageRoot,
            arguments: &["Tarnwick", "name=Grace Hopper"],
            mutating: true,
        },
        ResponseContract::VendorContactEdit => Shape {
            line: &["vendor", "contact", "edit"],
            subject: Subject::PackageRoot,
            arguments: &["Tarnwick", "1", "role=Sales"],
            mutating: true,
        },
        ResponseContract::VendorContactMove => Shape {
            line: &["vendor", "contact", "move"],
            subject: Subject::PackageRootWithThreeContacts,
            arguments: &["Tarnwick", "1", "position=2"],
            mutating: true,
        },
        ResponseContract::VendorContactRemove => Shape {
            line: &["vendor", "contact", "remove"],
            subject: Subject::PackageRoot,
            arguments: &["Tarnwick", "1"],
            mutating: true,
        },
        ResponseContract::Repair => Shape {
            line: &["repair"],
            subject: Subject::PackageRoot,
            arguments: &[],
            mutating: true,
        },
        ResponseContract::Upgrade => Shape {
            line: &["upgrade"],
            subject: Subject::PackageRoot,
            arguments: &[],
            mutating: true,
        },
        ResponseContract::IndexRebuild => Shape {
            line: &["index", "rebuild"],
            subject: Subject::PackageRoot,
            arguments: &[],
            mutating: true,
        },
    }
}

/// One prepared subject, with the tree kept alive for as long as the run needs it.
struct Prepared {
    _tree: TemporaryTree,
    /// What `--root` names for this command.
    root: PathBuf,
    /// The whole tree a dry run must leave byte-identical.
    ///
    /// **The Package rather than [`Self::root`]**, because an `order` command's `--root`
    /// names the Order Directory and the effects a dry run must not have are not all
    /// inside it: a mutation of one Order rewrites the Package's derived index and a
    /// removal can create the Removed Directory beside the Orders one. Snapshotting the
    /// Order Directory alone would watch the one place those do not appear. For `init`
    /// this is the unmanaged directory it was pointed at, which is where a Package would
    /// stand — and a dry run that created one is exactly the effect being ruled out.
    package_root: PathBuf,
    positional: Option<PathBuf>,
}

fn prepare(contract: ResponseContract, label: &str) -> Prepared {
    match shape(contract).subject {
        Subject::PackageRoot => {
            let (tree, root) = populated_package(label);
            Prepared {
                _tree: tree,
                package_root: root.clone(),
                root,
                positional: None,
            }
        }
        Subject::PackageRootWithThreeContacts => {
            let (tree, root) = package_with_three_contacts(label);
            Prepared {
                _tree: tree,
                package_root: root.clone(),
                root,
                positional: None,
            }
        }
        Subject::OrderRoot => {
            let (tree, root) = populated_package(label);
            let directory = order_directory(&root, "Laser_components");
            Prepared {
                _tree: tree,
                root: directory,
                package_root: root,
                positional: None,
            }
        }
        Subject::UnmanagedDirectory => {
            let (tree, root) = unmanaged_directory(label);
            Prepared {
                _tree: tree,
                package_root: root.clone(),
                root,
                positional: None,
            }
        }
        Subject::AdoptionCandidate => {
            let (tree, root, candidate) = package_with_an_adoption_candidate(label);
            Prepared {
                _tree: tree,
                package_root: root.clone(),
                root,
                positional: Some(candidate),
            }
        }
        Subject::NoCommand => unreachable_subject(),
    }
}

/// Never called: every loop below skips the one contract no command line reaches.
fn unreachable_subject() -> Prepared {
    let (tree, root) = unmanaged_directory("no-command");
    Prepared {
        _tree: tree,
        package_root: root.clone(),
        root,
        positional: None,
    }
}

/// The command line that reaches one contract, targeted at a prepared subject.
fn invocation(contract: ResponseContract, prepared: &Prepared) -> Invocation {
    invocation_at(contract, prepared, &prepared.root)
}

/// The same command line, targeted somewhere else.
///
/// `--root` is supplied once, here, rather than appended by a caller: naming it twice is a
/// usage error, so a check about what a command does with a root has to build the whole
/// command line rather than add to one.
fn invocation_at(
    contract: ResponseContract,
    prepared: &Prepared,
    root: &std::path::Path,
) -> Invocation {
    let shape = shape(contract);
    let mut run = Invocation::new(shape.line);
    if let Some(positional) = &prepared.positional {
        run = run.argument(positional);
    }
    for argument in shape.arguments {
        run = run.argument(argument);
    }
    run.rooted_at(root)
}

/// Every contract this executable can emit, except the one no command line reaches.
fn commanded() -> impl Iterator<Item = ResponseContract> {
    ResponseContract::REGISTERED
        .iter()
        .copied()
        .filter(|contract| shape(*contract).subject != Subject::NoCommand)
}

// ---------------------------------------------------------------------------
// What every command owes under `--json`.
// ---------------------------------------------------------------------------

/// Every command writes one document and nothing else, in the one envelope, under its own
/// identity.
///
/// **Asserted from real process streams**, because "exactly one UTF-8 document on standard
/// output and no incidental text" is a claim about what a process wrote and an internal
/// return value cannot stand in for one. `support::response` refuses a second document,
/// trailing text, or anything at all on standard error, so every run here makes that claim
/// as well as the ones written out below.
#[test]
fn every_command_answers_with_one_document_in_the_one_envelope() {
    for contract in commanded() {
        let label = format!("conformance-{}", contract.name().replace('.', "-"));
        let prepared = prepare(contract, &label);
        let outcome = invocation(contract, &prepared).argument("--json").run();
        let document = response(&outcome);
        let name = contract.name();

        assert_eq!(
            outcome.status,
            Some(0),
            "{name}: this run reaches an answer, and did not: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], name, "{name}");
        assert_eq!(document["contract_version"], 1, "{name}");

        let mut members: Vec<&str> = document
            .as_object()
            .unwrap_or_else(|| panic!("{name}: a response is a JSON object"))
            .keys()
            .map(String::as_str)
            .collect();
        members.sort_unstable();
        let mut expected = ENVELOPE;
        expected.sort_unstable();
        assert_eq!(members, expected, "{name}: all seven members, always");

        let text = outcome.stdout_text();
        let mut previous = 0;
        for member in ENVELOPE {
            let key = format!("\"{member}\":");
            let at = text
                .find(&key)
                .unwrap_or_else(|| panic!("{name}: `{member}` is written"));
            assert!(
                at > previous,
                "{name}: `{member}` is out of the envelope's order"
            );
            previous = at;
        }

        assert_eq!(document["ok"], true, "{name}: `ok` is the exit status");
        assert!(
            document["error"].is_null(),
            "{name}: an answer carries no error"
        );
        assert!(
            !document["data"].is_null(),
            "{name}: an answer carries its payload"
        );
        assert!(
            document["diagnostics"].is_array(),
            "{name}: an absent collection is present and empty"
        );
        assert_snake_case_keys(&document, name, "");
    }
}

/// Every key of every response is `snake_case`, wherever it is nested.
fn assert_snake_case_keys(value: &serde_json::Value, contract: &str, at: &str) {
    match value {
        serde_json::Value::Object(members) => {
            for (key, member) in members {
                assert!(
                    is_snake_case(key),
                    "{contract}: `{at}{key}` is not snake_case"
                );
                assert_snake_case_keys(member, contract, &format!("{at}{key}."));
            }
        }
        serde_json::Value::Array(members) => {
            for member in members {
                assert_snake_case_keys(member, contract, at);
            }
        }
        _ => {}
    }
}

fn is_snake_case(value: &str) -> bool {
    !value.is_empty()
        && value.starts_with(|character: char| character.is_ascii_lowercase())
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

/// Every stable code a caller branches on is `snake_case`, on a failure as on an answer.
///
/// The codes rather than every string, because a message, a path, and a Vendor Name are
/// text a human wrote and none of them is a value to branch on.
#[test]
fn every_stable_code_a_failure_carries_is_snake_case() {
    for contract in commanded() {
        let label = format!("codes-{}", contract.name().replace('.', "-"));
        let prepared = prepare(contract, &label);
        // A `--root` naming nothing at all, which every command refuses — the one failing
        // run every command has in common, so the codes can be read as a set.
        let nowhere = prepared.root.join("nothing-stands-here");
        let outcome = invocation_at(contract, &prepared, &nowhere)
            .argument("--json")
            .run();
        let document = response(&outcome);
        let name = contract.name();

        assert_eq!(
            document["contract"], name,
            "{name}: a failure keeps its identity"
        );
        assert_eq!(document["ok"], false, "{name}");
        assert!(
            document["data"].is_null(),
            "{name}: a failure carries no data"
        );
        for code in [&document["error"]["code"], &document["error"]["scope"]] {
            let text = code
                .as_str()
                .unwrap_or_else(|| panic!("{name}: a failure names its {code}"));
            assert!(is_snake_case(text), "{name}: `{text}` is not snake_case");
        }
        for identity in document["error"]["identities"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: identities is present and possibly empty"))
        {
            let kind = identity["kind"]
                .as_str()
                .expect("an identity names its kind");
            assert!(is_snake_case(kind), "{name}: `{kind}` is not snake_case");
        }
    }
}

/// No command searches an ancestor for its root.
///
/// **`--root` names the root exactly**, so a command pointed at a plain directory inside a
/// Package — or inside an Order Directory — fails rather than finding the Package one level
/// up. The Package above is what makes this a claim: without it, "found nothing" and
/// "searched nothing" are the same observation.
///
/// `init` is deliberately absent. Its subject is a directory that must *not* be a Package,
/// so a plain directory inside one is a perfectly good subject for it and initializing a
/// nested Package is not an ancestor search.
#[test]
fn no_command_finds_its_root_by_searching_an_ancestor() {
    for contract in commanded().filter(|contract| *contract != ResponseContract::Init) {
        let label = format!("ancestors-{}", contract.name().replace('.', "-"));
        let prepared = prepare(contract, &label);
        let inside = directory_that_is_not_a_root(&prepared.root);
        let outcome = invocation_at(contract, &prepared, &inside)
            .argument("--json")
            .run();
        let document = response(&outcome);
        let name = contract.name();

        assert_eq!(
            outcome.status,
            Some(1),
            "{name}: a plain directory is not a root, and no ancestor is consulted: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["ok"], false, "{name}");
    }
}

/// Every mutating command accepts `--dry-run` and leaves the Package byte-identical.
///
/// **Over the whole set, including the commands whose own ticket already tested it**: a
/// dry run that wrote is the defect this is about, and it is only a contract if every
/// mutation obeys it. The snapshot is of every byte under the Package rather than of a
/// directory listing, because a staging artifact left behind is exactly the effect a
/// listing would miss — and of [`Prepared::package_root`] rather than of what `--root`
/// named, because the index rewrite and the Removed Directory an `order` mutation could
/// leave stand outside its Order Directory.
#[test]
fn every_mutating_command_takes_a_dry_run_and_changes_nothing() {
    for contract in commanded().filter(|contract| shape(*contract).mutating) {
        let label = format!("dry-run-{}", contract.name().replace('.', "-"));
        let prepared = prepare(contract, &label);
        let before = managed_snapshot(&prepared.package_root);
        let outcome = invocation(contract, &prepared)
            .argument("--json")
            .argument("--dry-run")
            .run();
        let document = response(&outcome);
        let name = contract.name();

        assert_eq!(
            outcome.status,
            Some(0),
            "{name}: a dry run of a mutation that would commit answers: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], name, "{name}");
        assert_eq!(
            managed_snapshot(&prepared.package_root),
            before,
            "{name}: a dry run creates no directory, file, staging artifact or index rewrite"
        );
    }
}

/// The canonical record one command's commit replaces, beside which a staging name stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Staged {
    /// The Package's Vendor aggregate, which every Vendor and Contact mutation replaces.
    VendorRecord,
    /// The record of the Order this command's `--root` names.
    OrderRecord,
    /// The Package's derived index.
    DerivedIndex,
}

impl Staged {
    /// The stable code a commit refused for want of a staging name carries.
    ///
    /// **What says the refusal is the commit's.** Without it a command that began
    /// refusing earlier — at its Package load, or while planning — would still answer
    /// `ok: false` with a spelled path, and the claim below would go on passing over a
    /// condition it had stopped reaching.
    fn refusal_code(self) -> &'static str {
        match self {
            Self::VendorRecord => "vendor_mutation_failed",
            Self::OrderRecord => "order_mutation_failed",
            Self::DerivedIndex => "index_replacement_failed",
        }
    }
}

/// Whether an interrupted earlier write refuses a command at its commit, or the reason
/// this command reaches no commit for one to refuse.
///
/// **An unreached condition and an unwritten check must not look alike**, for the reason
/// [`Supply::Unreachable`] carries: a hole with a reason is a decision, and a hole without
/// one is an omission nobody has noticed yet.
#[derive(Clone, Copy, Debug)]
enum Interrupted {
    Refused(Staged),
    Unreached(&'static str),
}

/// What an interrupted earlier write does to each registered contract, exhaustively.
///
/// A match rather than a table, so a contract registered later does not compile until
/// somebody says which record its commit replaces — or why nothing it does can be refused
/// this way.
fn interrupted(contract: ResponseContract) -> Interrupted {
    match contract {
        // One Vendor aggregate is the whole Package's, and all eight mutations of a Vendor
        // or one of its Contacts replace it.
        ResponseContract::AddVendor
        | ResponseContract::VendorEdit
        | ResponseContract::VendorHide
        | ResponseContract::VendorUnhide
        | ResponseContract::VendorContactAdd
        | ResponseContract::VendorContactEdit
        | ResponseContract::VendorContactMove
        | ResponseContract::VendorContactRemove => Interrupted::Refused(Staged::VendorRecord),
        // A fact edit and the four Item mutations replace the one record of the Order they
        // are targeted at.
        ResponseContract::OrderEdit
        | ResponseContract::OrderItemAdd
        | ResponseContract::OrderItemEdit
        | ResponseContract::OrderItemMove
        | ResponseContract::OrderItemRemove => Interrupted::Refused(Staged::OrderRecord),
        ResponseContract::IndexRebuild => Interrupted::Refused(Staged::DerivedIndex),
        ResponseContract::Init => Interrupted::Unreached(
            "no Package stands where `init` is pointed, so it replaces no record of one",
        ),
        ResponseContract::AddOrder => Interrupted::Unreached(
            "it writes the record of an Order Directory that does not exist yet, so no \
             staging name inside one stands to be occupied before it",
        ),
        ResponseContract::AdoptOrder => Interrupted::Unreached(
            "a staging name already standing in the candidate is refused while the \
             adoption is planned, so there is no commit left to refuse",
        ),
        ResponseContract::OrderAlign | ResponseContract::OrderRemove => Interrupted::Unreached(
            "its commit relocates the whole Order Directory intact rather than replacing \
             any record inside it",
        ),
        ResponseContract::Repair | ResponseContract::OrderRepair => Interrupted::Unreached(
            "the command line that reaches this contract is the bare diagnosis, which \
             answers rather than commits",
        ),
        ResponseContract::Upgrade => Interrupted::Unreached(
            "a Package already at the newest version this build knows has nothing to \
             upgrade, so its commit replaces nothing",
        ),
        ResponseContract::Usage => {
            Interrupted::Unreached("no command line reaches it, so no commit of one is refused")
        }
        // The reads. A read replaces no record, so nothing an interrupted write left
        // behind refuses one.
        ResponseContract::PackageReadme
        | ResponseContract::PackageShow
        | ResponseContract::PackageListOrders
        | ResponseContract::PackageListVendors
        | ResponseContract::PackageValuesOrderingInstitution
        | ResponseContract::PackageValuesOrderer
        | ResponseContract::PackageValuesProject
        | ResponseContract::PackageValuesOrderGroup
        | ResponseContract::OrderShow
        | ResponseContract::OrderReadme
        | ResponseContract::OrderValidate
        | ResponseContract::VendorShow
        | ResponseContract::Validate
        | ResponseContract::IndexStatus => {
            Interrupted::Unreached("a read replaces no record, so it has no commit to refuse")
        }
    }
}

/// Every refusal an interrupted earlier write reaches names its record in the Package's
/// own spelling for it.
///
/// **The claim [`every_recorded_path_carries_the_packages_own_spelling_for_it`] cannot
/// make.** Each contract records one refusal, so every other refusal condition is checked
/// by nothing at all — and this one is nobody's cell. A directory standing at a managed
/// record's staging name is what an interrupted earlier write leaves; it is reachable from
/// the command line, and it refuses the *commit* rather than the plan, which is a different
/// half of every command from the half a `rejected` cell records. That is exactly where a
/// Vendor mutation reported the record it could not replace with no spelling while the
/// Package stood around it, and an acceptance gate is what found it.
///
/// So the same claim is made here over the response a real process wrote, contract by
/// contract, with the reason written down wherever the condition reaches no commit.
#[test]
fn every_refusal_an_interrupted_write_reaches_carries_the_packages_own_spelling_for_it() {
    let mut refused = false;
    for contract in ResponseContract::REGISTERED {
        let name = contract.name();
        let record = match interrupted(*contract) {
            Interrupted::Refused(record) => record,
            Interrupted::Unreached(reason) => {
                assert!(
                    reason.len() > 20,
                    "{name} says nothing about why an interrupted write refuses no commit of it"
                );
                continue;
            }
        };
        let label = format!("interrupted-{}", name.replace('.', "-"));
        let prepared = prepare(*contract, &label);
        occupy_the_staging_name_beside(&staged_record(record, &prepared));

        let outcome = invocation(*contract, &prepared).argument("--json").run();
        let document = response(&outcome);
        assert_eq!(
            outcome.status,
            Some(1),
            "{name}: a commit that cannot stage its replacement is refused: {}",
            outcome.stdout_text()
        );
        assert_eq!(
            document["ok"], false,
            "{name}: `ok` and the exit status are one fact"
        );
        assert_eq!(
            document["error"]["code"],
            record.refusal_code(),
            "{name}: this run is refused at its commit rather than before it"
        );

        let root = prepared
            .package_root
            .to_str()
            .expect("a temporary test tree is named in Unicode");
        assert_eq!(
            document["context"]["package_root"].as_str(),
            Some(root),
            "{name}: the Package this refusal happened under is the one it names"
        );
        let mut reported = Vec::new();
        collect_reported_paths(&document, &mut reported);
        assert!(
            !reported.is_empty(),
            "{name}: a refusal about a record it could not replace names that record"
        );
        assert_spelled_by_the_package_at(&reported, root, name);
        refused = true;
    }
    // Named rather than counted, for the reason the recorded walk gives: a loop that
    // reached no commit at all would make every claim above vacuously.
    assert!(
        refused,
        "the condition reaches a commit somewhere, or this checks nothing"
    );
}

/// The record whose replacement one command's commit performs, in the prepared subject.
fn staged_record(record: Staged, prepared: &Prepared) -> PathBuf {
    match record {
        Staged::VendorRecord => prepared.package_root.join(".spielorders/vendors.json"),
        // `--root` names the Order Directory for every command whose record this is.
        Staged::OrderRecord => prepared.root.join(".spielorders/order.json"),
        Staged::DerivedIndex => prepared.package_root.join(".spielorders/index.json"),
    }
}

/// A directory at the staging name beside one managed record.
///
/// The shape an interrupted earlier write leaves: a managed replacement stages beside its
/// destination and refuses an occupied name, so the next one fails before it has committed
/// anything. A directory rather than a file, because nothing about this condition should
/// depend on what the occupant holds.
fn occupy_the_staging_name_beside(record: &Path) {
    let name = record
        .file_name()
        .and_then(|name| name.to_str())
        .expect("a managed record has a basename named in Unicode");
    std::fs::create_dir(record.with_file_name(format!(".{name}.spielorders-stage")))
        .expect("the record's staging name is occupied");
}

/// No read takes `--dry-run`, because a read has no mutation to withhold.
#[test]
fn no_read_offers_to_withhold_a_mutation_it_does_not_have() {
    for contract in commanded().filter(|contract| !shape(*contract).mutating) {
        let label = format!("no-dry-run-{}", contract.name().replace('.', "-"));
        let prepared = prepare(contract, &label);
        let outcome = invocation(contract, &prepared)
            .argument("--json")
            .argument("--dry-run")
            .run();
        let name = contract.name();

        assert_eq!(
            outcome.status,
            Some(2),
            "{name}: `--dry-run` answers a question a read does not ask: {}",
            outcome.stdout_text()
        );
        assert_eq!(
            response(&outcome)["contract"],
            ResponseContract::Usage.name(),
            "{name}"
        );
    }
}

/// `--quiet` says nothing on success, for every command.
#[test]
fn quiet_says_nothing_on_success_for_every_command() {
    for contract in commanded() {
        let label = format!("quiet-{}", contract.name().replace('.', "-"));
        let prepared = prepare(contract, &label);
        let outcome = invocation(contract, &prepared).argument("--quiet").run();
        let name = contract.name();

        assert_eq!(outcome.status, Some(0), "{name}: {}", outcome.stderr_text());
        assert_eq!(outcome.stdout, b"", "{name}: no output on success");
    }
}

/// Every pair of global options one exclusion covers, over every command.
///
/// **Read out of [`GLOBAL_EXCLUSIONS`] rather than transcribed**, so the next global
/// option added to one of those sets is swept across the whole command surface without
/// anyone remembering that this test is here.
fn excluded_pairs() -> impl Iterator<Item = (&'static str, &'static str)> {
    GLOBAL_EXCLUSIONS.iter().flat_map(|exclusion| {
        exclusion.iter().enumerate().flat_map(|(index, first)| {
            exclusion[index + 1..]
                .iter()
                .map(move |second| (*first, *second))
        })
    })
}

/// What a command line writes after one global option, where naming it takes a value.
///
/// A registry of exclusions says which options exclude each other and not how a caller
/// writes one, so an option missing here fails by name below rather than being skipped.
/// `--root` is absent because every line in this sweep already names it once.
fn value_after(option: &str) -> Option<&'static str> {
    match option {
        "json" | "quiet" | "verbose" => None,
        "uuid" => Some(NEWEST_ORDER_ID),
        other => panic!(
            "`--{other}` excludes another global option and nothing here says what a \
             command line writes after it"
        ),
    }
}

/// No command runs while its command line names two options one exclusion covers.
///
/// **A usage error rather than a silent precedence**, and the one shared identity rather
/// than the command's own: parsing failed, so nothing about the subject was attempted and
/// no command has a response to give.
///
/// That, and only that, is what this asserts over the whole command surface. A pair whose
/// options one command does not both declare — `--uuid` is the `order` family's — is
/// refused there as an unrecognized option instead, which is the same exit and the same
/// identity for a different reason. **Which refusal each pair earns, and in which
/// placements, is asserted where that pair is written**: the exclusion itself is decided
/// once for every command, so it is the placements rather than the commands that can hide
/// one.
///
/// Every run is still rooted at a prepared subject, even though the claim is that none of
/// them reaches one. A command line that named no root would inherit this process's
/// working directory, so the day an exclusion regressed the failure would be a mutation of
/// the repository this test is checked out in rather than a red test.
#[test]
fn no_command_runs_while_its_line_names_two_excluded_globals() {
    for contract in commanded() {
        let label = format!("excluded-globals-{}", contract.name().replace('.', "-"));
        let prepared = prepare(contract, &label);
        let name = contract.name();

        for (first, second) in excluded_pairs() {
            let mut run = invocation(contract, &prepared);
            for option in [first, second] {
                // `--root` is written once, by `invocation`, and naming it twice is a
                // usage error of its own — so the exclusion that names it needs only its
                // partner written out here.
                if option == "root" {
                    continue;
                }
                run = run.argument(format!("--{option}"));
                if let Some(value) = value_after(option) {
                    run = run.argument(value);
                }
            }
            // A pair of two human modes then stands beside `--json`, which is a third
            // option of the same exclusion: which two the sentence names is that test's
            // subject, and this one's is that the command did not run.
            if first != "json" && second != "json" {
                run = run.argument("--json");
            }
            let outcome = run.run();

            assert_eq!(
                outcome.status,
                Some(2),
                "{name}, `--{first}` with `--{second}`: nothing was attempted: {}",
                outcome.stdout_text()
            );
            assert_eq!(
                response(&outcome)["contract"],
                ResponseContract::Usage.name(),
                "{name}, `--{first}` with `--{second}`"
            );
        }
    }
}

/// `--version` is one answer for the executable, in every mode.
#[test]
fn the_executable_reports_its_version_as_an_answer() {
    for mode in [None, Some("--json"), Some("--quiet"), Some("--verbose")] {
        let mut run = Invocation::new(&["--version"]);
        if let Some(mode) = mode {
            run = run.argument(mode);
        }
        let outcome = run.run();

        assert_eq!(outcome.status, Some(0), "{mode:?}");
        assert_eq!(outcome.stderr, b"", "{mode:?}: a reference is an answer");
        assert!(
            outcome.stdout_text().contains("spielorders"),
            "{mode:?}: {}",
            outcome.stdout_text()
        );
    }
}
