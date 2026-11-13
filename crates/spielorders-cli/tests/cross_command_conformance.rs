//! Every command against one contract, checked as a set rather than one command at a time.
//!
//! **The inventory is the evidence.** [`DECIDED_COMMANDS`] transcribes the command tree the
//! decisions name — not the tree the executable happens to have — and it is read in both
//! directions: a decided command the executable lacks fails here, and so does a registered
//! contract no decided command claims. A command decided to arrive later is recorded with
//! what it waits for — the milestone that owns it, or the condition its decision names —
//! and asserted *absent*, so one shipped early is a failure rather than a surprise.
//!
//! **Every claim below is made over the whole set.** The tables are exhaustive matches on
//! the suite's own `ResponseContract`, so a contract added to it is not merely encouraged to
//! conform — the suite does not compile until somebody says how it is reached, what cells it
//! supplies, and whether it mutates, and then it is held to every rule here without anyone
//! remembering to write a test for it. A command the executable ships under a contract the
//! suite does not list is caught where it records cells, by the walk that fails for a
//! fixture directory no listed contract claims.
//!
//! What is deliberately *not* here is any command's own bytes: `tests/response_contract.rs`
//! compares each recorded cell, and this file says which cells there should be.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders agent
//! skill contract.

use crate::support::ResponseContract;
use crate::support::{
    ENVELOPE, Invocation, SOME_ORDER_ID, TemporaryTree, directory_that_is_not_a_root, entry_names,
    file_outside_any_package, fixture_directory, initialized_package, link_to, managed_snapshot,
    material_answered, order_directory, package_with_an_adoption_candidate,
    package_with_an_unused_vendor, package_with_three_contacts, populated_package, response,
    supporting_material_provider, unmanaged_directory,
};
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
    /// Decided to arrive with what is named — another milestone, or the condition the
    /// decision ties it to — and absent until then. The check below asserts the executable
    /// does not answer it: shipping one early would expose a surface whose engine does not
    /// exist yet.
    Absent(&'static str),
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
        &["vendor", "delete"],
        Disposition::Own(ResponseContract::VendorDelete),
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
    (
        &["search", "orders"],
        Disposition::Own(ResponseContract::SearchOrders),
    ),
    (
        &["search", "vendors"],
        Disposition::Own(ResponseContract::SearchVendors),
    ),
    (
        &["search", "supporting-material"],
        Disposition::Own(ResponseContract::SearchSupportingMaterial),
    ),
    (&["validate"], Disposition::Own(ResponseContract::Validate)),
    (&["repair"], Disposition::Own(ResponseContract::Repair)),
    (
        &["upgrade"],
        Disposition::Absent("the first metadata version after 1"),
    ),
    (
        &["index", "status"],
        Disposition::Own(ResponseContract::IndexStatus),
    ),
    (
        &["index", "rebuild"],
        Disposition::Own(ResponseContract::IndexRebuild),
    ),
    (
        &["agent", "provision"],
        Disposition::Own(ResponseContract::AgentProvision),
    ),
    (
        &["agent", "defaults"],
        Disposition::Own(ResponseContract::AgentDefaults),
    ),
];

/// Every decided command this milestone delivers is a command line the executable answers,
/// and every command still waiting is one the executable does not.
///
/// **The forward direction of the inventory.** Generated `--help` is what asks the
/// question: a command line the parser recognizes renders its reference and exits `0`, and
/// one it does not is a usage error at exit `2`. That is also the whole of what M8's
/// generated references cite, so it is confirmed here for every command rather than
/// sampled.
#[test]
fn every_decided_command_is_delivered_or_absent_until_what_it_waits_for() {
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
            Disposition::Absent(until) => {
                assert_eq!(
                    outcome.status,
                    Some(2),
                    "{line:?} waits for {until} and this executable answers it: {}",
                    outcome.stdout_text()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The golden-fixture matrix, over every contract at once.
// ---------------------------------------------------------------------------

/// Every response kind a contract's fixture directory can hold a cell for.
///
/// `usage-error` is not among them and never will be: it is not any command's cell, it
/// carries the one shared identity, and `tests/response_contract.rs`
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
        ResponseContract::AgentProvision => [
            ("success", Supply::Recorded),
            (
                "empty-result",
                Supply::Unreachable("an unchanged provisioning still reports its preserved paths"),
            ),
            (
                "warning",
                Supply::Unreachable(
                    "guidance creation has no warning-producing mutation or index refresh",
                ),
            ),
            ("invalid", Supply::Recorded),
            ("rejected", Supply::Recorded),
        ],
        ResponseContract::AgentDefaults => [
            ("success", Supply::Recorded),
            (
                "empty-result",
                Supply::Unreachable("a resource query always names its installed directory"),
            ),
            (
                "warning",
                Supply::Unreachable("a resource query loads no Package and has no findings"),
            ),
            (
                "invalid",
                Supply::Unreachable("a resource query loads no Package to judge invalid"),
            ),
            ("rejected", Supply::Recorded),
        ],
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
        | ResponseContract::SearchOrders
        | ResponseContract::SearchVendors
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
        | ResponseContract::VendorDelete
        | ResponseContract::VendorContactAdd
        | ResponseContract::VendorContactEdit
        | ResponseContract::VendorContactMove
        | ResponseContract::VendorContactRemove => MUTATION,
        // `init`'s subject is a directory that must *not* be a Package, so it reads none
        // and has no untrusted subject to report; and the core defines no initialization
        // warning at all, which its `Never` warning type is what says.
        ResponseContract::Init => [
            ("success", Supply::Recorded),
            (
                "empty-result",
                Supply::Unreachable("init answers with the Package it just created"),
            ),
            (
                "warning",
                Supply::Unreachable(
                    "an initialization warns with `Never`, and an initialization assesses \
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
        // The nonempty response is captured with the real index; its replay does not
        // depend on the host index having reached a temporary Package.
        ResponseContract::SearchSupportingMaterial => [
            ("success", Supply::Recorded),
            ("empty-result", Supply::Recorded),
            ("warning", Supply::Recorded),
            ("invalid", Supply::Recorded),
            (
                "rejected",
                Supply::Unreachable("a read refuses no mutation"),
            ),
        ],
        // The warning every other mutation carries is that its canonical write succeeded
        // and the derived refresh did not — and the derived index is this command's whole
        // subject, so a rebuild that could not replace it wrote nothing and is `rejected`.
        // Its warning type is `Never`, which is the core saying so.
        ResponseContract::IndexRebuild => [
            ("success", Supply::Recorded),
            (
                "empty-result",
                Supply::Unreachable("a mutation reports what it changed"),
            ),
            (
                "warning",
                Supply::Unreachable(
                    "a rebuild warns with `Never`: this command's whole \
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
        let mut expected = Vec::new();
        let mut unreachable = Vec::new();
        for (cell, supply) in matrix(*contract) {
            match supply {
                Supply::Recorded => expected.push(format!("{cell}.json")),
                Supply::Unreachable(reason) => unreachable.push(format!("{cell}: {reason}")),
            }
        }
        expected.sort();
        let directory = fixture_directory(*contract);
        assert_eq!(
            entry_names(&directory),
            expected,
            "{} records a different set of cells from the one the matrix supplies, in {}; \
             the matrix says the rest are unreachable: {unreachable:?}",
            contract.name(),
            directory.display()
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
/// A recorded identity is numbered in place of the UUID it was, and the numbering replaces
/// only lowercase hyphenated UUIDs — so a cell carrying one has had this checked for it by
/// the comparison that recorded it.
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
            if value.starts_with("<ID-") && value.ends_with('>') {
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
/// `tests/response_contract.rs` compares the fixture tree with executable output, except
/// Supporting Material answer cells, which replay prepared snapshots through the production
/// response encoder. These checks cover every recorded cell rather than only selected runs.
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
    /// A Package Root with no Order, so a Supporting Material search has nowhere to look
    /// and asks no platform index: the sweeps never query the machine's own index.
    PackageRootWithoutOrders,
    /// The same Root, with three Contacts on the Vendor it offers. A `move` within a
    /// collection of one is the same position twice, which the core answers as an
    /// unchanged mutation without replacing anything — so the one command that moves a
    /// Contact needs somewhere to move it before its commit is a commit at all.
    PackageRootWithThreeContacts,
    /// The same Root, also holding a Vendor no Order references, the one kind a deletion
    /// removes rather than refuses.
    PackageRootWithAnUnusedVendor,
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
    /// Installed resources: no Package root is accepted.
    InstalledResources,
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
        ResponseContract::AgentDefaults => {
            read(&["agent", "defaults"], Subject::InstalledResources)
        }
        ResponseContract::AgentProvision => Shape {
            line: &["agent", "provision"],
            subject: Subject::PackageRoot,
            arguments: &[],
            mutating: true,
        },
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
        ResponseContract::VendorDelete => Shape {
            line: &["vendor", "delete"],
            subject: Subject::PackageRootWithAnUnusedVendor,
            arguments: &["Elderglass Optics"],
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
        ResponseContract::IndexRebuild => Shape {
            line: &["index", "rebuild"],
            subject: Subject::PackageRoot,
            arguments: &[],
            mutating: true,
        },
        // The keyword is a term one Order of the fixture Package carries and no Vendor
        // does, and the Vendor search's is the other way round: each reaches an answer,
        // and each reaches one carrying something.
        ResponseContract::SearchOrders => Shape {
            line: &["search", "orders"],
            subject: Subject::PackageRoot,
            arguments: &["laser"],
            mutating: false,
        },
        ResponseContract::SearchVendors => Shape {
            line: &["search", "vendors"],
            subject: Subject::PackageRoot,
            arguments: &["tarnwick"],
            mutating: false,
        },
        ResponseContract::SearchSupportingMaterial => Shape {
            line: &["search", "supporting-material"],
            subject: Subject::PackageRootWithoutOrders,
            arguments: &["zirconium"],
            mutating: false,
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
        Subject::PackageRootWithoutOrders => {
            let (tree, root) = initialized_package(label);
            Prepared {
                _tree: tree,
                package_root: root.clone(),
                root,
                positional: None,
            }
        }
        Subject::PackageRootWithAnUnusedVendor => {
            let (tree, root) = package_with_an_unused_vendor(label);
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
        Subject::NoCommand | Subject::InstalledResources => unreachable_subject(),
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

/// Ordinary fixture commands answer; Supporting Material validates its runtime outcome.
fn answers_here(
    contract: ResponseContract,
    outcome: &crate::support::Outcome,
    document: &serde_json::Value,
) -> bool {
    if contract == ResponseContract::SearchSupportingMaterial {
        material_answered(outcome, document, supporting_material_provider())
    } else {
        true
    }
}

/// Every Package-targeted contract. Installed-resource queries are covered separately
/// because they accept no Package root.
fn commanded() -> impl Iterator<Item = ResponseContract> {
    ResponseContract::REGISTERED
        .iter()
        .copied()
        .filter(|contract| {
            !matches!(
                shape(*contract).subject,
                Subject::NoCommand | Subject::InstalledResources
            )
        })
}

/// Every command line a caller can write, with the contract it answers under.
///
/// **Wider than [`commanded`]**, which walks contracts: a command line that renders
/// another's contract has no entry of its own there, so a claim made per contract passes
/// over `order item list` and `vendor contact list` entirely. A claim about what a
/// *command* does with the root it was pointed at is about the line a caller writes, and
/// both of those are lines a caller writes.
///
/// The subject and the arguments come from the contract the line answers under, which is
/// what makes them the same run under a different spelling — the property
/// [`DECIDED_COMMANDS`] records by calling one `Shared`.
fn command_lines() -> impl Iterator<Item = (&'static [&'static str], ResponseContract)> {
    DECIDED_COMMANDS
        .iter()
        .filter_map(|(line, disposition)| match disposition {
            Disposition::Own(contract) | Disposition::Shared(contract) => Some((*line, *contract)),
            Disposition::Absent(_) => None,
        })
        .filter(|(_, contract)| {
            !matches!(
                shape(*contract).subject,
                Subject::NoCommand | Subject::InstalledResources
            )
        })
}

/// One command line as the caller writes it, targeted somewhere else.
fn line_at(
    line: &[&str],
    contract: ResponseContract,
    prepared: &Prepared,
    root: &std::path::Path,
) -> Invocation {
    let shape = shape(contract);
    let mut run = Invocation::new(line);
    if let Some(positional) = &prepared.positional {
        run = run.argument(positional);
    }
    for argument in shape.arguments {
        run = run.argument(argument);
    }
    run.rooted_at(root)
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
        let answers = answers_here(contract, &outcome, &document);

        assert_eq!(
            outcome.status,
            Some(if answers { 0 } else { 1 }),
            "{name}: this run reaches {}, and did not: {}",
            if answers {
                "an answer"
            } else {
                "the state that prevented one"
            },
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

        assert_eq!(document["ok"], answers, "{name}: `ok` is the exit status");
        assert_eq!(
            document["error"].is_null(),
            answers,
            "{name}: an answer carries no error, and a failure carries one"
        );
        assert_eq!(
            !document["data"].is_null(),
            answers,
            "{name}: an answer carries its payload, and a failure carries none"
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
        let text = document["error"]["code"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: a failure names its code: {document}"));
        assert!(is_snake_case(text), "{name}: `{text}` is not snake_case");
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
/// nested Package is not an ancestor search. `agent provision` accepts any directory, so it
/// shows the same claim by writing into exactly the directory it names.
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

        if contract == ResponseContract::AgentProvision {
            assert_eq!(outcome.status, Some(0), "{name}: {}", outcome.stdout_text());
            assert!(inside.join(".agents").is_dir(), "{name}");
            assert!(!prepared.root.join(".agents").exists(), "{name}");
            continue;
        }
        assert_eq!(
            outcome.status,
            Some(1),
            "{name}: a plain directory is not a root, and no ancestor is consulted: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["ok"], false, "{name}");
    }
}

/// No command follows a link named as its root, and every one of them says so the same way.
///
/// **Links are never followed and never adopted**, so a path that is one is refused
/// wherever it is named — and the whole point of naming it a root is that the command
/// would otherwise act on whatever stands behind it. `order remove` through a link
/// disposes of a whole Order Directory; `package show` through one reports a Package the
/// caller did not name.
///
/// Swept over every command rather than tested on a few, because one code for all of them
/// is the claim: a command that grew its own answer would be a second thing a consumer has
/// to branch on, and a command that grew no answer at all would still be following links.
#[test]
fn no_command_follows_a_link_named_as_its_root() {
    for (line, contract) in command_lines() {
        let name = line.join(" ");
        let label = format!("root-link-{}", name.replace(' ', "-"));
        let prepared = prepare(contract, &label);
        let (_link_tree, link) = link_to(&label, &prepared.root);
        // A trailing separator makes the OS follow the link, so it is named both ways.
        for link in [link.clone(), link.join("")] {
            let outcome = line_at(line, contract, &prepared, &link)
                .argument("--json")
                .run();
            let document = response(&outcome);

            assert_eq!(
                outcome.status,
                Some(1),
                "{name} {}: a link is never followed: {}",
                link.display(),
                outcome.stdout_text()
            );
            assert_eq!(
                document["error"]["code"],
                "root_is_a_link",
                "{name} {}: {document}",
                link.display()
            );
            assert_eq!(
                document["error"]["detail"]["path"],
                link.to_str().expect("the test path is UTF-8"),
                "{name}: the refusal names the link, not what stands behind it"
            );
        }
    }
}

/// Nothing behind a link named as a root is read, renamed, disposed of, or edited.
///
/// The companion claim to the codes above: a refusal that answered correctly and had
/// already acted would be worse than no refusal at all. Every mutating command is run
/// through a link at the Package it would change, and the Package is compared byte for
/// byte — the same snapshot the dry-run sweep uses, for the same reason.
#[test]
fn nothing_behind_a_link_named_as_a_root_is_touched() {
    for contract in commanded().filter(|contract| shape(*contract).mutating) {
        let label = format!("root-link-effects-{}", contract.name().replace('.', "-"));
        let prepared = prepare(contract, &label);
        let (_link_tree, link) = link_to(&label, &prepared.root);
        let before = managed_snapshot(&prepared.package_root);

        let outcome = invocation_at(contract, &prepared, &link)
            .argument("--json")
            .run();

        assert_eq!(outcome.status, Some(1), "{}", contract.name());
        assert_eq!(
            managed_snapshot(&prepared.package_root),
            before,
            "{}: nothing behind the link was created, renamed, replaced, or disposed of",
            contract.name()
        );
    }
}

/// A wrong-kind `order` root reaches one code, and no message names a port operation.
///
/// **A file named as an Order Directory is one refusal, not two.** The reads reported it
/// as a Package they could not read — quoting an internal port operation and a path under
/// the file that cannot exist — while the validating commands answered that the root is
/// not an Order Directory. A consumer branching on the code got a different answer
/// depending on which `order` command it ran.
#[test]
fn a_wrong_kind_order_root_reaches_one_code_across_the_family() {
    for (line, contract) in
        command_lines().filter(|(_, contract)| shape(*contract).subject == Subject::OrderRoot)
    {
        let name = line.join(" ");
        let label = format!("order-root-file-{}", name.replace(' ', "-"));
        let prepared = prepare(contract, &label);
        let (_file_tree, file) = file_outside_any_package(&label);
        let outcome = line_at(line, contract, &prepared, &file)
            .argument("--json")
            .run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(1),
            "{name}: a file is not an Order Directory: {}",
            outcome.stdout_text()
        );
        assert_eq!(
            document["error"]["code"], "not_an_order_directory",
            "{name}: {document}"
        );
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
        // Bare `repair` is a diagnosis, which reports cases rather than a planned mutation.
        if !matches!(
            contract,
            ResponseContract::Repair | ResponseContract::OrderRepair
        ) {
            assert_eq!(
                document["data"]["committed"], false,
                "{name}: the planned report says it committed nothing"
            );
        }
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
            Self::VendorRecord | Self::OrderRecord => "commit_failed",
            Self::DerivedIndex => "commit_failed",
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
        | ResponseContract::VendorDelete
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
        ResponseContract::AgentDefaults => {
            Interrupted::Unreached("a resource location query does not write anything")
        }
        ResponseContract::AgentProvision => Interrupted::Unreached(
            "an occupied guidance staging path refuses planning before a commit exists",
        ),
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
        | ResponseContract::IndexStatus
        | ResponseContract::SearchOrders
        | ResponseContract::SearchVendors
        | ResponseContract::SearchSupportingMaterial => {
            Interrupted::Unreached("a read replaces no record, so it has no commit to refuse")
        }
    }
}

/// Every refusal an interrupted earlier write reaches names its record in the Package's
/// own spelling for it.
///
/// A directory standing at a managed record's staging name is what an interrupted earlier
/// write leaves; it is reachable from the command line, and it refuses the *commit* rather
/// than the plan, which is a different half of every command from the half a `rejected`
/// cell records. The claim is made over the response a real process wrote, contract by
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

        let staged = staged_record(record, &prepared);
        let relative = staged
            .strip_prefix(&prepared.package_root)
            .expect("the record is inside its Package");
        assert_eq!(
            document["error"]["detail"]["path"],
            relative
                .to_str()
                .expect("a temporary test tree is named in Unicode"),
            "{name}: a refusal about a record it could not replace names that record as \
             its Package spells it"
        );
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

/// `--quiet` says nothing on success, for every command, and still says what failed.
///
/// The second half is the one a caller depends on where a command cannot answer: silence on
/// both streams would read as an answer, so an error is retained on standard error whatever
/// the output mode.
#[test]
fn quiet_says_nothing_on_success_for_every_command() {
    for contract in commanded() {
        let label = format!("quiet-{}", contract.name().replace('.', "-"));
        let prepared = prepare(contract, &label);
        let outcome = invocation(contract, &prepared).argument("--quiet").run();
        let name = contract.name();
        let answers = if contract == ResponseContract::SearchSupportingMaterial {
            assert!(matches!(outcome.status, Some(0 | 1)));
            if supporting_material_provider().is_none() {
                assert_eq!(outcome.status, Some(1));
            }
            outcome.status == Some(0)
        } else {
            true
        };

        assert_eq!(
            outcome.status,
            Some(if answers { 0 } else { 1 }),
            "{name}: {}",
            outcome.stderr_text()
        );
        assert_eq!(outcome.stdout, b"", "{name}: no output on success");
        assert_eq!(
            outcome.stderr.is_empty(),
            answers,
            "{name}: `--quiet` withholds an answer and keeps an error"
        );
    }
}

/// Every set of options one command line may name at most one of: the decided output
/// modes, and the two ways an `order` command names one Order.
///
/// The parser declares each on the group that owns it, and its unit tests sweep every
/// pair those groups declare; this is the suite's statement of the decided sets.
const EXCLUSIONS: [&[&str]; 2] = [&["json", "quiet", "verbose"], &["root", "uuid"]];

/// Every pair of options one exclusion covers, over every command.
fn excluded_pairs() -> Vec<(&'static str, &'static str)> {
    let mut pairs = Vec::new();
    for exclusion in EXCLUSIONS {
        for (index, first) in exclusion.iter().enumerate() {
            for second in &exclusion[index + 1..] {
                pairs.push((*first, *second));
            }
        }
    }
    pairs
}

/// What a command line writes after one excluded option, where naming it takes a value.
///
/// A group says which options exclude each other and not what a caller writes after one,
/// so an option missing here fails by name below rather than being skipped. `--root` is
/// absent because every line in this sweep already names it once.
fn value_after(option: &str) -> Option<&'static str> {
    match option {
        "json" | "quiet" | "verbose" => None,
        "uuid" => Some(SOME_ORDER_ID),
        other => panic!(
            "`--{other}` excludes another option and nothing here says what a command \
             line writes after it"
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
/// options one command does not both declare — `--uuid` is the `order` commands' — is
/// refused there as an unrecognized option instead, which is the same exit and the same
/// identity for a different reason. **Which refusal each pair earns is asserted where that
/// pair is declared**, in the unit tests beside the group that declares it.
///
/// Every run is still rooted at a prepared subject, even though the claim is that none of
/// them reaches one. A command line that named no root would inherit this process's
/// working directory, so the day an exclusion regressed the failure would be a mutation of
/// the repository this test is checked out in rather than a red test.
#[test]
fn no_command_runs_while_its_line_names_two_excluded_options() {
    for contract in commanded() {
        let label = format!("excluded-options-{}", contract.name().replace('.', "-"));
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

/// `--version` is one answer for the executable.
///
/// It names no command, so it carries no output mode either: the modes belong to the
/// commands, and `--version` is a reference request rather than a modifier.
#[test]
fn the_executable_reports_its_version_as_an_answer() {
    let outcome = Invocation::new(&["--version"]).run();

    assert_eq!(outcome.status, Some(0));
    assert_eq!(outcome.stderr, b"", "a reference is an answer");
    assert!(
        outcome.stdout_text().contains("spielorders"),
        "{}",
        outcome.stdout_text()
    );
}
