//! Shared fixtures for the `spielorders` executable's own test target.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders
//! package topology.

#![allow(dead_code)]

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// Every envelope member, in the one order every response carries them.
///
/// A fixed key set in a fixed order is what lets a consumer check a response without
/// knowing which command produced it. Here rather than in either file that asserts it,
/// because `tests/response_contract.rs` reads it as the shape one command's recorded bytes
/// must have and `tests/cross_command_conformance.rs` reads it as the shape every command's
/// live document must have — one fact, and two copies of it would be two envelopes the day
/// one of them was edited.
pub const ENVELOPE: [&str; 7] = [
    "contract",
    "contract_version",
    "ok",
    "context",
    "data",
    "diagnostics",
    "error",
];

/// The Supporting Material index provider this build ships, in its own name, or `None` where
/// its platform has none.
///
/// **Asked of the build rather than stated here**, because a suite that decided this for
/// itself would assert what it had assumed — and a build that shipped no provider where its
/// platform has one is the defect the milestone's provider ticket was opened for.
/// `supporting_material_search::this_build_ships_the_provider_its_platform_has` is what makes
/// reading it safe: it holds this answer against the platform the run is on, so every claim
/// that branches on it branches on a checked fact.
///
/// Here rather than in one of them, because `search supporting-material` is the one command
/// whose answer a platform decides: the golden comparison, the whole-surface sweeps, and that
/// command's own suite all have to know which answer this build gives.
pub fn supporting_material_provider() -> Option<&'static str> {
    spielorders_cli::effects::supporting_material_index()
        .map(spielorders_core::ports::SupportingMaterialIndex::name)
}

/// Distinguishes trees claimed within one process. The whole suite is one test binary,
/// so two tests that pass the same label share a process id, and a clock read is not
/// guaranteed to separate two threads that reach it together.
static TREE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct TemporaryTree(PathBuf);

impl TemporaryTree {
    pub fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock follows the Unix epoch")
            .as_nanos();
        let sequence = TREE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "spielorders-cli-{label}-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("temporary test root is created");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryTree {
    /// Cleanup reports rather than panics.
    ///
    /// A panic here would run while the thread is already unwinding from the assertion
    /// that actually failed, which aborts the process and takes the real failure message
    /// with it — so a test whose tree cannot be removed would report a cleanup problem
    /// instead of its own defect. Every fixture in this module builds trees of awkward
    /// shapes on purpose, so this is a live hazard rather than a theoretical one.
    ///
    /// The leak is bounded and visible: one directory under the system temporary
    /// directory, named for the test that left it, with the reason on standard error.
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!(
                "warning: the temporary test tree {} could not be removed: {error}",
                self.0.display()
            );
        }
    }
}

/// A symbolic link to `target`, standing in a tree of its own.
///
/// **Its own tree rather than beside the target**, because a link inside a Package's
/// Orders Directory is a disallowed entry the Package then diagnoses — a second thing for
/// a test about a link *named as a root* to trip over.
pub fn link_to(label: &str, target: &Path) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let link = tree.path().join("link");
    std::os::unix::fs::symlink(target, &link).expect("the link is created");
    (tree, link)
}

/// An ordinary file, standing in a tree of its own.
///
/// The wrong kind of thing to name as any root: no Package surrounds it and nothing about
/// it is a directory.
pub fn file_outside_any_package(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let file = tree.path().join("notes.txt");
    std::fs::write(&file, b"not a root\n").expect("the fixture file is written");
    (tree, file)
}

pub fn entry_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .expect("the directory can be listed")
        .map(|entry| {
            entry
                .expect("the entry can be read")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// An ordinary directory that is not a Package, and its canonical path.
///
/// The subject `init` requires, and the one fixture in this file that deliberately
/// contains nothing: a Package is what the command under test is supposed to produce.
/// The canonical spelling comes back because that is what the core resolves the selected
/// root to, and what a response therefore carries.
pub fn unmanaged_directory(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let path = std::fs::canonicalize(tree.path()).expect("the temporary directory resolves");
    (tree, path)
}

/// A committed SpielOrder Package at its default layout, and its canonical root.
///
/// Built by the core capability that owns Package initialization rather than by
/// writing managed bytes here, so a test Package is a real one and this file decides
/// no Package rule. Its filesystem effect is the executable's own production adapter,
/// which is the only one this crate has.
pub fn initialized_package(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = spielorders_core::initialization::plan_package_initialization(
        tree.path(),
        &spielorders_core::initialization::InitializationOptions::default(),
        &spielorders_cli::effects::PlatformFileSystem,
    )
    .expect("the test Package can be planned");
    assert!(matches!(
        spielorders_core::initialization::commit_package_initialization(
            plan,
            &spielorders_cli::effects::PlatformFileSystem
        ),
        spielorders_core::mutation::MutationReport::Committed { .. }
    ));
    // The canonical spelling, because that is what the core reports back and what
    // macOS turns `/var/folders/...` into.
    let root = std::fs::canonicalize(tree.path()).expect("the test Package root resolves");
    (tree, root)
}

/// A Package whose Orders Directory is a configured path several levels below the Root,
/// holding one Order, and that Order's Directory.
///
/// **The layout `--root` and the locator have to agree about.** Under the default layout
/// an Order Directory is two levels below the Root, so a targeting rule that counted a
/// fixed two parents would be right by accident; here it is four, and the Order's own
/// `package_root` locator is the only thing that knows it. This is the shape of the M3
/// defect that shipped — a repair that counted two parents and refreshed a stranger's
/// directory — so it is exercised rather than assumed to behave like the default.
pub fn package_with_a_nested_orders_directory(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::{
        initialization::{
            InitializationOptions, commit_package_initialization, plan_package_initialization,
        },
        mutation::MutationReport,
    };

    let tree = TemporaryTree::new(label);
    let plan = plan_package_initialization(
        tree.path(),
        &InitializationOptions::new("archive/2026/Orders", "Removed"),
        &PlatformFileSystem,
    )
    .expect("the nested test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &PlatformFileSystem),
        MutationReport::Committed { .. }
    ));
    let root = std::fs::canonicalize(tree.path()).expect("the test Package root resolves");
    let directory = one_order_created_in(&root, "Nested order");
    (tree, root, directory)
}

/// One Order created in an existing Package, and its Order Directory.
///
/// Created by the core capability that owns Order creation, so the Order is a real one
/// and this file decides no Package rule — including where the Order Directory goes,
/// which follows the Package's own configured Orders Directory wherever that is.
pub fn one_order_created_in(root: &Path, title: &str) -> PathBuf {
    order_created_in(root, title, NEWEST_ORDER_ID)
}

/// The same, under an identity the caller names, for a Package that already holds Orders.
pub fn order_created_in(root: &Path, title: &str, order_id: &str) -> PathBuf {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::{
        creation::{OrderCreationInput, commit_order_creation, plan_order_creation},
        mutation::MutationReport,
    };

    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new(title).date_ordered("2026-08-22"),
        &FixedCalendar,
        &PlatformFileSystem,
    )
    .expect("the fixture Order can be planned");
    let report = commit_order_creation(
        plan,
        &mut QueuedUuids::new(&[order_id]),
        &PlatformFileSystem,
    );
    let MutationReport::Committed { result, .. } = report else {
        panic!("the fixture Order commits: {report:?}");
    };
    result
        .order()
        .paths()
        .directory()
        .absolute_path()
        .to_path_buf()
}

/// A calendar fixed to one date.
///
/// A Package built by these fixtures has to be byte-stable, because golden fixtures
/// compare its response against recorded bytes. Today's date is not.
pub struct FixedCalendar;

impl spielorders_core::ports::LocalCalendar for FixedCalendar {
    fn today(&self) -> spielorders_core::ports::LocalDate {
        spielorders_core::ports::LocalDate {
            year: 2026,
            month: 8,
            day: 22,
        }
    }
}

/// Identities handed out in a fixed order, for the same reason.
pub struct QueuedUuids(Vec<String>);

impl QueuedUuids {
    pub fn new(values: &[&str]) -> Self {
        Self(
            values
                .iter()
                .rev()
                .map(|value| (*value).to_owned())
                .collect(),
        )
    }
}

impl spielorders_core::ports::UuidGenerator for QueuedUuids {
    fn generate_v4(&mut self) -> String {
        self.0.pop().expect("the fixture queued enough identities")
    }
}

/// The Vendor [`populated_package`] offers, and the one it hides.
///
/// Named constants rather than literals in each test because an Order referencing the
/// Hidden Vendor is the point of the second one: hiding governs what is offered for new
/// use and never what is resolved.
pub const OFFERED_VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
pub const HIDDEN_VENDOR_ID: &str = "1c9a5b7e-3d44-4f21-8a6c-9b0e2d7f4a13";
pub const NEWEST_ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
pub const OLDEST_ORDER_ID: &str = "3f2a1c05-7b19-4e63-9d84-5a0c6e8b2f71";
/// A third identity, for a fixture that needs one more Order than the Package has.
pub const THIRD_ORDER_ID: &str = "c4d81b62-0e57-4a90-b3f5-6d2170ae9c38";

/// A Package with everything `package show` has to report.
///
/// Two Orders with Items and totals, an offered Vendor with a Contact and a Hidden
/// Vendor, a README with text, and all four Reusable Free-Entry Value lists populated —
/// including one value used twice, so a usage count of `1` cannot pass for the count.
/// The second Order references the Hidden Vendor, which is what makes "a Hidden Vendor
/// still resolves" a claim about this Package rather than a hypothetical.
///
/// Built entirely through core capabilities, so a fixture Package is a real one and this
/// file decides no Package rule. Hiding happens last because every plan re-reads the
/// Package it is about.
pub fn populated_package(label: &str) -> (TemporaryTree, PathBuf) {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::{
        creation::{OrderCreationInput, commit_order_creation, plan_order_creation},
        mutation::MutationReport,
        order_edits::{ItemAssignments, commit_order_item_mutation, plan_order_item_add},
        vendors::{
            ContactAssignments, VendorAssignments, commit_vendor_contact_mutation,
            commit_vendor_mutation, plan_vendor_contact_add, plan_vendor_creation,
            plan_vendor_hide,
        },
    };

    let (tree, root) = initialized_package(label);

    let mut vendor_uuids = QueuedUuids::new(&[OFFERED_VENDOR_ID, HIDDEN_VENDOR_ID]);
    for assignments in [
        VendorAssignments::new()
            .assign("name", "Tarnwick")
            .assign("phone", "+1 555 0123")
            .assign("email", "sales@example.com")
            .assign("website", "https://example.com")
            .assign("address", "100 Example Avenue\nAnytown, ST 00000")
            .assign("purchasing_notes", "Quote required above $5000."),
        VendorAssignments::new().assign("name", "Nethermoor"),
    ] {
        let plan = plan_vendor_creation(&root, &assignments, &PlatformFileSystem)
            .expect("the fixture Vendor can be planned");
        assert!(matches!(
            commit_vendor_mutation(plan, &mut vendor_uuids, &PlatformFileSystem),
            MutationReport::Committed { .. }
        ));
    }

    let contact = plan_vendor_contact_add(
        &root,
        OFFERED_VENDOR_ID,
        &ContactAssignments::new()
            .assign("name", "Katherine Johnson")
            .assign("role", "Accounts")
            .assign("email", "accounts@example.com"),
        None,
        &PlatformFileSystem,
    )
    .expect("the fixture Contact can be planned");
    assert!(matches!(
        commit_vendor_contact_mutation(contact, &PlatformFileSystem),
        MutationReport::Committed { .. }
    ));

    let orders = [
        (
            NEWEST_ORDER_ID,
            OrderCreationInput::new("Laser components")
                .date_ordered("2026-08-22")
                .vendor_name("Tarnwick")
                .orderer("Ada Lovelace")
                .ordering_institution("Northwind")
                .currency("USD")
                .project("Telescope")
                .order_group("Summer build"),
            vec![
                ItemAssignments::new()
                    .assign("description", "Fiber collimator")
                    .assign("quantity", "4")
                    .assign("unit_price", "3.0")
                    .assign("catalog_number", "F220APC"),
                ItemAssignments::new()
                    .assign("description", "Mirror mount")
                    .assign("quantity", "2")
                    .assign("unit_price", "84.5"),
            ],
        ),
        (
            OLDEST_ORDER_ID,
            OrderCreationInput::new("Mirror mounts")
                .date_ordered("2026-08-21")
                .vendor_name("Nethermoor")
                .orderer("Ada Lovelace")
                .project("Telescope"),
            vec![
                ItemAssignments::new()
                    .assign("description", "Post holder")
                    .assign("quantity", "6")
                    .assign("unit_price", "12.25"),
            ],
        ),
    ];

    for (order_id, input, items) in orders {
        let plan = plan_order_creation(&root, &input, &FixedCalendar, &PlatformFileSystem)
            .expect("the fixture Order can be planned");
        let report = commit_order_creation(
            plan,
            &mut QueuedUuids::new(&[order_id]),
            &PlatformFileSystem,
        );
        let MutationReport::Committed { result, .. } = report else {
            panic!("the fixture Order commits: {report:?}");
        };
        let directory = result
            .order()
            .paths()
            .directory()
            .absolute_path()
            .to_path_buf();
        for item in items {
            let plan = plan_order_item_add(&directory, &item, None, &PlatformFileSystem)
                .expect("the fixture Item can be planned");
            assert!(matches!(
                commit_order_item_mutation(plan, &PlatformFileSystem),
                MutationReport::Committed { .. }
            ));
        }
    }

    // Last, because every plan above re-reads the Package and a Hidden Vendor is not
    // offered for a new Order.
    let hide = plan_vendor_hide(&root, HIDDEN_VENDOR_ID, &PlatformFileSystem)
        .expect("the fixture Vendor can be hidden");
    assert!(matches!(
        commit_vendor_mutation(
            hide,
            &mut QueuedUuids::new(&["00000000-0000-4000-8000-00000000ffff"]),
            &PlatformFileSystem
        ),
        MutationReport::Committed { .. }
    ));

    std::fs::write(root.join("README.md"), b"Telescope purchasing.\n")
        .expect("the fixture Package README is written");

    (tree, root)
}

/// [`populated_package`] with two more Contacts on the Vendor it offers.
///
/// **Three Contacts is the smallest collection a position claim can be made against**: with
/// one, every position that names a Contact names the same one, so an `edit` at position 2
/// and an `edit` at position 1 cannot be told apart — and a `move` has nowhere to go. The
/// three carry different optional facts, so a projection that reported a role where an
/// email belongs is a difference rather than a coincidence.
///
/// Separate from [`populated_package`] rather than an adjustment to it, for the reason
/// [`package_with_one_order_priced`] is separate: that fixture's response is recorded in
/// golden fixtures, and a test that needed one more Contact would move bytes no ticket had
/// a reason to move.
pub fn package_with_three_contacts(label: &str) -> (TemporaryTree, PathBuf) {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::{
        mutation::MutationReport,
        vendors::{ContactAssignments, commit_vendor_contact_mutation, plan_vendor_contact_add},
    };

    let (tree, root) = populated_package(label);
    for assignments in [
        ContactAssignments::new()
            .assign("name", "Grace Hopper")
            .assign("role", "Engineering")
            .assign("phone", "+1 555 0142"),
        ContactAssignments::new()
            .assign("name", "Alan Turing")
            .assign("email", "alan@example.com"),
    ] {
        let plan = plan_vendor_contact_add(
            &root,
            OFFERED_VENDOR_ID,
            &assignments,
            None,
            &PlatformFileSystem,
        )
        .expect("the fixture Contact can be planned");
        assert!(matches!(
            commit_vendor_contact_mutation(plan, &PlatformFileSystem),
            MutationReport::Committed { .. }
        ));
    }
    (tree, root)
}

/// The same, with the derived Package Index's staging name occupied.
///
/// See [`package_with_a_blocked_index_refresh`], whose condition this is — recorded against
/// a Vendor with three Contacts because a Contact mutation that commits nothing refreshes
/// nothing, so the committed-with-warnings outcome needs a collection there is something to
/// move within.
pub fn package_with_three_contacts_and_a_blocked_index_refresh(
    label: &str,
) -> (TemporaryTree, PathBuf) {
    let (tree, root) = package_with_three_contacts(label);
    block_index_refresh(&root);
    (tree, root)
}

/// [`populated_package`] with one more Item on the Order it holds first, and that Order's
/// Directory.
///
/// **Three Items is the smallest collection a position claim can be made against**, for the
/// reason [`package_with_three_contacts`] holds three Contacts: with one, every position
/// that names an Item names the same one, and a `move` has nowhere to go. The three carry
/// different Descriptions, Quantities, and Unit Prices, and only one carries a Catalog
/// Number, so an Item confused for its neighbour or a total taken from the wrong row
/// changes an assertion instead of surviving it.
///
/// Separate from [`populated_package`] rather than an adjustment to it, for the reason
/// [`package_with_three_contacts`] is separate: that fixture's response is recorded in
/// golden fixtures, and a test that needed one more Item would move bytes no ticket had a
/// reason to move.
pub fn package_with_three_items(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::{
        mutation::MutationReport,
        order_edits::{ItemAssignments, commit_order_item_mutation, plan_order_item_add},
    };

    let (tree, root) = populated_package(label);
    let directory = order_directory(&root, "Laser_components");
    let plan = plan_order_item_add(
        &directory,
        &ItemAssignments::new()
            .assign("description", "Optical breadboard")
            .assign("quantity", "1")
            .assign("unit_price", "220.0"),
        None,
        &PlatformFileSystem,
    )
    .expect("the fixture Item can be planned");
    assert!(matches!(
        commit_order_item_mutation(plan, &PlatformFileSystem),
        MutationReport::Committed { .. }
    ));
    (tree, root, directory)
}

/// The same, with the derived Package Index's staging name occupied.
///
/// See [`package_with_a_blocked_index_refresh`], whose condition this is — recorded against
/// an Order with three Items because an Item mutation that commits nothing refreshes
/// nothing, so the committed-with-warnings outcome needs a collection there is something to
/// move within.
pub fn package_with_three_items_and_a_blocked_index_refresh(
    label: &str,
) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root, directory) = package_with_three_items(label);
    block_index_refresh(&root);
    (tree, root, directory)
}

/// [`populated_package`] with one Order's Title edited, so its Directory name has
/// drifted from the name its facts now project.
///
/// **Drift is what alignment is for**, and it is ordinary allowed state: editing a fact
/// deliberately leaves the directory alone, so a Package only ever has a drifted Order
/// because somebody edited one. That is exactly how this is built — through the core
/// capability that owns an Order fact edit — rather than by renaming the directory here,
/// which would be a fixture deciding what an aligned name is.
///
/// Returns the Package Root and the drifted Order's Directory, still at its old name.
pub fn package_with_a_drifted_order(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let directory = drift_one_order(&root);
    (tree, root, directory)
}

/// The same, with the derived Package Index's staging name occupied.
///
/// See [`package_with_a_blocked_index_refresh`], whose condition this is — over a drifted
/// Order because an alignment that renames nothing refreshes nothing, so the
/// committed-with-warnings outcome needs a directory there is something to move.
pub fn package_with_a_drifted_order_and_a_blocked_index_refresh(
    label: &str,
) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root, directory) = package_with_a_drifted_order(label);
    block_index_refresh(&root);
    (tree, root, directory)
}

/// The same, with an ordinary directory already standing at the aligned name.
///
/// The collision alignment refuses rather than resolving with an invented suffix. The
/// occupied name is found by asking the core what this Order's facts now project, so the
/// fixture decides no naming rule.
///
/// Returns the Package Root, the drifted Order's Directory, and the occupied name.
pub fn package_with_an_occupied_aligned_name(
    label: &str,
) -> (TemporaryTree, PathBuf, PathBuf, PathBuf) {
    let (tree, root, directory) = package_with_a_drifted_order(label);
    let occupied = projected_alignment(&directory);
    std::fs::create_dir(&occupied).expect("the aligned name is occupied");
    (tree, root, directory, occupied)
}

/// The two opaque Supporting Material sentinels the canonical contract corpus supplies.
///
/// Resolved from `contracts/fixtures/assets` rather than written here, so the bytes a
/// whole-directory move is asserted to preserve are the same bytes the core's own suite
/// preserves. They are deliberately different from each other and must never be parsed as
/// metadata. See `contracts/fixtures/README.md`.
pub const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
pub const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

/// Both sentinels placed in one Order Directory, one of them nested.
///
/// **What a whole-directory move has to carry.** One at the top and one inside a
/// subdirectory, because a move that enumerated children could plausibly carry the first
/// and lose the second — and neither is ever read, hashed, or rewritten by anything under
/// test. The Order README is given text for the same reason: it travels with the
/// directory like any other file in it.
///
/// The tree a whole-directory move has to carry is compared with
/// [`managed_snapshot`], which reports every byte under a directory: the same entries,
/// the same contents, nothing added and nothing dropped.
pub fn supporting_material_in(directory: &Path) {
    let nested = directory.join("Quotes");
    std::fs::create_dir(&nested).expect("the Supporting Material subdirectory is created");
    std::fs::write(directory.join("supporting-a.bin"), SUPPORTING_A)
        .expect("the first sentinel is written");
    std::fs::write(nested.join("supporting-b.bin"), SUPPORTING_B)
        .expect("the second sentinel is written");
    std::fs::write(directory.join("README.md"), b"Ordered by telephone.\n")
        .expect("the Order README is given text");
}

/// [`populated_package`] with an ordinary directory already standing where a removal
/// would put one of its Orders.
///
/// The collision a disposition refuses rather than resolving with an invented suffix. The
/// Removed Orders Directory is created here on purpose: it is created lazily by the
/// removal itself, and a collision can only be observed inside one that is already there.
///
/// Returns the Package Root, the Order's Directory, and the occupied destination.
pub fn package_with_an_occupied_removed_name(
    label: &str,
) -> (TemporaryTree, PathBuf, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let directory = order_directory(&root, "Laser_components");
    let removed = removed_orders_directory(&root);
    std::fs::create_dir(&removed).expect("the Removed Orders Directory is created early");
    let occupied = removed.join(
        directory
            .file_name()
            .expect("an Order Directory has a basename"),
    );
    std::fs::create_dir(&occupied).expect("the destination name is occupied");
    (tree, root, directory, occupied)
}

/// One of a Package's Orders retitled, leaving its Directory at its old name.
fn drift_one_order(root: &Path) -> PathBuf {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::{
        mutation::MutationReport,
        order_edits::{OrderAssignments, commit_order_edit, plan_order_edit},
    };

    let directory = order_directory(root, "Laser_components");
    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser heads"),
        &FixedCalendar,
        &PlatformFileSystem,
    )
    .expect("the fixture Order fact edit can be planned");
    assert!(matches!(
        commit_order_edit(plan, &PlatformFileSystem),
        MutationReport::Committed { .. }
    ));
    assert!(
        directory.is_dir(),
        "a fact edit leaves the Order Directory exactly where it was"
    );
    directory
}

/// The path an alignment of `directory` would rename it to, as the core projects it.
///
/// Asked of the core rather than spelled here, so a test that expects the aligned name
/// and a fixture that occupies it are both about the name alignment actually reaches for.
pub fn projected_alignment(directory: &Path) -> PathBuf {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::alignment::plan_order_alignment;

    let plan = plan_order_alignment(directory, &PlatformFileSystem)
        .expect("the drifted Order's alignment can be planned");
    assert!(
        plan.changes_directory_name(),
        "the fixture Order has drifted from the name its facts project"
    );
    plan.destination().absolute_path().to_path_buf()
}

/// A Package holding one Vendor that has only a Name.
///
/// **The empty answer for both `vendor` reads at once**: no Contacts, no Order referencing
/// it, and none of the optional shared facts. In each case the answer is that there is
/// nothing there, not that there is no answer — and a Vendor with only a Name is the
/// ordinary result of `add-vendor "Nethermoor"`, not a contrived one.
pub fn package_with_an_unreferenced_vendor(label: &str) -> (TemporaryTree, PathBuf) {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::{
        mutation::MutationReport,
        vendors::{VendorAssignments, commit_vendor_mutation, plan_vendor_creation},
    };

    let (tree, root) = initialized_package(label);
    let plan = plan_vendor_creation(
        &root,
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &PlatformFileSystem,
    )
    .expect("the fixture Vendor can be planned");
    assert!(matches!(
        commit_vendor_mutation(
            plan,
            &mut QueuedUuids::new(&[OFFERED_VENDOR_ID]),
            &PlatformFileSystem
        ),
        MutationReport::Committed { .. }
    ));
    (tree, root)
}

/// A Package holding one Order whose Items carry exactly the money a test is about.
///
/// Each entry is one Item's `quantity` and `unit_price`, as the canonical assignments a
/// caller would supply, so a test that means "an Order totalling zero" or "a total with
/// four decimal places" says so in the numbers rather than in a comment. The Items are
/// otherwise featureless and the Order names no Vendor: the subject is the arithmetic.
///
/// Separate from [`populated_package`] rather than an adjustment to it, because that
/// fixture's response is recorded in golden fixtures and a test that needed one more
/// Order would move bytes no ticket had a reason to move.
pub fn package_with_one_order_priced(
    label: &str,
    items: &[(&str, &str)],
) -> (TemporaryTree, PathBuf) {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::{
        creation::{OrderCreationInput, commit_order_creation, plan_order_creation},
        mutation::MutationReport,
        order_edits::{ItemAssignments, commit_order_item_mutation, plan_order_item_add},
    };

    let (tree, root) = initialized_package(label);

    let plan = plan_order_creation(
        &root,
        &OrderCreationInput::new("Priced order").date_ordered("2026-08-22"),
        &FixedCalendar,
        &PlatformFileSystem,
    )
    .expect("the fixture Order can be planned");
    let report = commit_order_creation(
        plan,
        &mut QueuedUuids::new(&[NEWEST_ORDER_ID]),
        &PlatformFileSystem,
    );
    let MutationReport::Committed { result, .. } = report else {
        panic!("the fixture Order commits: {report:?}");
    };
    let directory = result
        .order()
        .paths()
        .directory()
        .absolute_path()
        .to_path_buf();

    for (position, (quantity, unit_price)) in items.iter().enumerate() {
        let assignments = ItemAssignments::new()
            .assign("description", format!("Item {}", position + 1))
            .assign("quantity", *quantity)
            .assign("unit_price", *unit_price);
        let plan = plan_order_item_add(&directory, &assignments, None, &PlatformFileSystem)
            .expect("the fixture Item can be planned");
        assert!(matches!(
            commit_order_item_mutation(plan, &PlatformFileSystem),
            MutationReport::Committed { .. }
        ));
    }

    (tree, root)
}

/// One of a Package's Order Directories, found by a fragment of its projected basename.
///
/// Found rather than spelled, for the reason [`package_with_a_damaged_order`] finds its
/// own: the core composes an Order Directory's basename from the Order's own facts, so a
/// literal here would make every `order` test depend on a naming rule that is not theirs.
pub fn order_directory(root: &Path, basename_fragment: &str) -> PathBuf {
    let orders = root.join("Orders");
    entry_names(&orders)
        .into_iter()
        .find(|name| name.contains(basename_fragment))
        .map(|name| orders.join(name))
        .unwrap_or_else(|| {
            panic!("the fixture Package has an Order Directory naming {basename_fragment}")
        })
}

/// [`populated_package`] with one Order Directory whose record is unreadable.
///
/// The damage is deliberately the whole record rather than one field: a directory under
/// the Orders Directory with no readable Order record is one Damaged Order, and the
/// Package around it is still a current candidate — Order-local damage does not damage
/// the Package, which is exactly what the placeholder is for.
pub fn package_with_a_damaged_order(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    // Found rather than spelled: the core composes an Order Directory's basename from
    // the Order's own facts, so hard-coding it here would make this fixture depend on a
    // naming rule that is not this ticket's.
    let orders = root.join("Orders");
    let damaged = entry_names(&orders)
        .into_iter()
        .find(|name| name.contains("Mirror_mounts"))
        .map(|name| orders.join(name))
        .expect("the fixture Package has the Order this damages");
    std::fs::write(damaged.join(".spielorders/order.json"), b"{ not json")
        .expect("the fixture Order record is damaged");
    (tree, root, damaged)
}

/// A Package holding one Order with no Items and no README.
///
/// **The empty answer for all three `order` reads at once**, which is why it is one
/// fixture rather than three: `order show` and `order item list` report a collection that
/// is present and empty, and `order readme` reports the conventional path with nothing
/// standing at it. In each case the answer is that there is nothing there, not that there
/// is no answer.
///
/// Returns the Package Root and the Order's Directory.
pub fn package_with_an_empty_order(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = package_with_one_order_priced(label, &[]);
    let directory = order_directory(&root, "Priced_order");
    let readme = directory.join("README.md");
    if readme.is_file() {
        std::fs::remove_file(&readme).expect("the Order README is removed");
    }
    (tree, root, directory)
}

/// [`populated_package`] with one Order's Date Ordered a day that does not exist.
///
/// **The Damaged Order whose record still parses**, which is the one that has diagnostics
/// to report: `2026-02-30` is a well-formed `YYYY-MM-DD` string, so the record
/// deserializes and the Package load quarantines the Order on semantic validation
/// instead. An Order whose record is not JSON at all never reaches a load, so it has no
/// diagnostics — the two are different answers and both are reachable.
///
/// Returns the Package Root and the damaged Order's Directory.
pub fn package_with_an_impossible_date(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let damaged = order_directory(&root, "Mirror_mounts");
    let record = damaged.join(".spielorders/order.json");
    let text = std::fs::read_to_string(&record).expect("the fixture Order record can be read");
    let rewritten = text.replace(
        "\"date_ordered\": \"2026-08-21\"",
        "\"date_ordered\": \"2026-02-30\"",
    );
    assert_ne!(text, rewritten, "the Date Ordered member was found");
    std::fs::write(&record, rewritten).expect("the rewritten Order record is written");
    (tree, root, damaged)
}

/// [`populated_package`] with one Order's conventional README a directory.
///
/// The one warning an `order` read can produce: the Order itself is healthy and its facts
/// are the answer, while the document that stands where its README belongs is not a
/// readable one. A shape rather than a permission, because `chmod 000` does not stop a
/// suite running as root and this does.
///
/// Returns the Package Root and that Order's Directory.
pub fn package_with_an_unreadable_order_readme(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let directory = order_directory(&root, "Laser_components");
    let readme = directory.join("README.md");
    if readme.is_file() {
        std::fs::remove_file(&readme).expect("the plain Order README is removed");
    }
    std::fs::create_dir(&readme).expect("a directory takes the Order README's place");
    (tree, root, directory)
}

/// [`populated_package`] with one Order's technical locator rewritten on disk, or removed.
///
/// **No core capability can produce this.** `package_root` is a managed technical field
/// that every Order mutation rejects, and the core computes the correct value whenever it
/// creates an Order Directory or aligns one. The condition is therefore written into the
/// record the core produced — one member's text and nothing else — because it is exactly
/// the shape a Package has after an Order Directory is moved by hand or its Orders
/// Directory is reconfigured to another depth, which is the most ordinary damage a Package
/// suffers.
///
/// Returns the Package Root, the Order Directory whose locator was rewritten, and the
/// sibling Order Directory that was left alone — the sibling being what makes "does not
/// fail unrelated Orders in the same Package" a claim about this Package.
pub fn package_with_one_locator_rewritten(
    label: &str,
    locator: Option<&str>,
) -> (TemporaryTree, PathBuf, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let broken = order_directory(&root, "Mirror_mounts");
    let sibling = order_directory(&root, "Laser_components");
    let record = broken.join(".spielorders/order.json");
    let text = std::fs::read_to_string(&record).expect("the fixture Order record can be read");
    let rewritten = text.replace(
        "  \"package_root\": \"../..\",\n",
        &match locator {
            Some(locator) => format!("  \"package_root\": \"{locator}\",\n"),
            None => String::new(),
        },
    );
    assert_ne!(
        text, rewritten,
        "the locator member was found and rewritten"
    );
    std::fs::write(&record, rewritten).expect("the rewritten Order record is written");
    (tree, root, broken, sibling)
}

/// Two SpielOrder Packages, one nested inside the other, and an Order of the inner one
/// whose locator names the *outer* Package.
///
/// This is the mismatched locator as distinct from the merely stale one: the place the
/// locator names is a real SpielOrders Root, and it holds no such Order. Returns the outer
/// Root, the inner Root, and the Order Directory.
pub fn order_whose_locator_names_another_package(
    label: &str,
) -> (TemporaryTree, PathBuf, PathBuf, PathBuf) {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::{
        creation::{OrderCreationInput, commit_order_creation, plan_order_creation},
        initialization::{
            InitializationOptions, commit_package_initialization, plan_package_initialization,
        },
        mutation::MutationReport,
    };

    let (tree, outer) = initialized_package(label);
    let inner_path = outer.join("inner");
    std::fs::create_dir(&inner_path).expect("the inner Package directory is created");
    let plan = plan_package_initialization(
        &inner_path,
        &InitializationOptions::default(),
        &PlatformFileSystem,
    )
    .expect("the inner test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &PlatformFileSystem),
        MutationReport::Committed { .. }
    ));
    let inner = std::fs::canonicalize(&inner_path).expect("the inner Package root resolves");

    let plan = plan_order_creation(
        &inner,
        &OrderCreationInput::new("Inner order").date_ordered("2026-08-22"),
        &FixedCalendar,
        &PlatformFileSystem,
    )
    .expect("the inner fixture Order can be planned");
    let report = commit_order_creation(
        plan,
        &mut QueuedUuids::new(&[NEWEST_ORDER_ID]),
        &PlatformFileSystem,
    );
    let MutationReport::Committed { result, .. } = report else {
        panic!("the inner fixture Order commits: {report:?}");
    };
    let directory = result
        .order()
        .paths()
        .directory()
        .absolute_path()
        .to_path_buf();

    // One segment too many, so the locator resolves past the inner Root and onto the
    // outer one — a Root that is real and holds no such Order.
    let record = directory.join(".spielorders/order.json");
    let text = std::fs::read_to_string(&record).expect("the fixture Order record can be read");
    let rewritten = text.replace(
        "  \"package_root\": \"../..\",\n",
        "  \"package_root\": \"../../..\",\n",
    );
    assert_ne!(text, rewritten, "the locator member was found");
    std::fs::write(&record, rewritten).expect("the rewritten Order record is written");

    (tree, outer, inner, directory)
}

/// [`populated_package`] with one Order's Project spelled differently on disk.
///
/// The two Orders' records then carry `Telescope` and `  teleSCOPE  `, which
/// differ only in letter case and in surrounding whitespace. **No core capability can
/// produce this**: `canonicalize_reusable_value` matches a proposed value against the
/// spellings already in use and writes the existing one, so every value a SpielOrders
/// mutation writes is already canonical. The condition is therefore written into the
/// record the core produced — one field's text and nothing else — because it is the shape
/// an adopted or externally edited Package has, and it is the shape the read-side
/// grouping rule exists for.
pub fn package_with_a_differently_spelled_value(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = populated_package(label);
    let orders = root.join("Orders");
    let order = entry_names(&orders)
        .into_iter()
        .find(|name| name.contains("Mirror_mounts"))
        .map(|name| orders.join(name).join(".spielorders/order.json"))
        .expect("the fixture Package has the Order this respells");
    let record = std::fs::read_to_string(&order).expect("the fixture Order record can be read");
    let respelled = record.replace(
        "\"project\": \"Telescope\"",
        "\"project\": \"  teleSCOPE  \"",
    );
    assert_ne!(
        record, respelled,
        "the Project field was found and replaced"
    );
    std::fs::write(&order, respelled).expect("the respelled Order record is written");
    (tree, root)
}

/// [`populated_package`] with *every* Order Directory's record unreadable.
///
/// Two Damaged Orders rather than one, which is what makes a claim about the order they
/// are presented in mean anything: one placeholder is in every order at once. The healthy
/// list is then empty, so this fixture answers only the Damaged half — the healthy half
/// is [`populated_package`], whose two Orders are in the opposite order to their
/// directory names.
pub fn package_with_every_order_damaged(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = populated_package(label);
    let orders = root.join("Orders");
    let names = entry_names(&orders);
    assert_eq!(
        names.len(),
        2,
        "the fixture Package has two Order Directories"
    );
    for name in names {
        std::fs::write(
            orders.join(name).join(".spielorders/order.json"),
            b"{ not json",
        )
        .expect("the fixture Order record is damaged");
    }
    (tree, root)
}

/// A Package whose marker declares a metadata version this build does not interpret.
///
/// The records themselves are untouched, so the Package is not damaged in any other
/// way: the one thing wrong with it is the version it declares, which is a state with
/// its own command rather than a repair.
pub fn package_at_an_uninterpretable_version(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = initialized_package(label);
    std::fs::write(
        root.join(".spielorders/version.json"),
        b"{\n  \"metadata_version\": 999\n}\n",
    )
    .expect("the fixture marker is written");
    (tree, root)
}

/// A Package whose Vendor records name one Vendor twice.
///
/// Package-wide canonical damage rather than Order-local: the load can read the Package
/// and cannot trust it, so it answers with diagnostics and no contents.
pub fn package_with_duplicate_vendor_names(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = initialized_package(label);
    std::fs::write(
        root.join(".spielorders/vendors.json"),
        format!(
            "{{\n  \"schema_version\": 1,\n  \"vendors\": [\n    \
             {{\n      \"vendor_id\": \"{OFFERED_VENDOR_ID}\",\n      \
             \"name\": \"Tarnwick\",\n      \"contacts\": []\n    }},\n    \
             {{\n      \"vendor_id\": \"{HIDDEN_VENDOR_ID}\",\n      \
             \"name\": \"tarnwick\",\n      \"contacts\": []\n    }}\n  ]\n}}\n"
        ),
    )
    .expect("the fixture Vendor records are written");
    (tree, root)
}

/// [`populated_package`] with the derived Package Index's staging name occupied.
///
/// **A real refresh failure, driven by the filesystem.** A managed replacement stages
/// beside its destination and `write_new` refuses an occupied path, so a directory
/// standing at the index's staging name makes the index rebuild that follows a canonical
/// write fail — while the canonical write itself, which stages beside `vendors.json`, is
/// untouched. That is the one condition the committed-with-warnings outcome needs, and it
/// cannot be produced by asking a core capability to misbehave.
///
/// A shape rather than a permission, for the reason every other fixture here avoids one:
/// `chmod 000` does not stop a suite running as root and this does. The staging name is
/// the core's own; a rename there is a change this fixture is supposed to notice.
pub fn package_with_a_blocked_index_refresh(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = populated_package(label);
    block_index_refresh(&root);
    (tree, root)
}

/// The same, for a Package with nothing in it yet.
pub fn initialized_package_with_a_blocked_index_refresh(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = initialized_package(label);
    block_index_refresh(&root);
    (tree, root)
}

fn block_index_refresh(root: &Path) {
    std::fs::create_dir(root.join(".spielorders/.index.json.spielorders-stage"))
        .expect("the index staging name is occupied");
}

// The preparations that put a Package's derived index into a state other than current.
// Shared because more than one command answers what state the derived index is in, and a
// second set of preparations is the shape in which two commands come to be asked about
// different Packages and their answers compared anyway.

/// Rewrites one Order record byte for byte, so the canonical input is newer than the index
/// that projected it and nothing about the record itself has changed.
pub fn touch_an_order_record(root: &Path) {
    let record = order_directory(root, "Laser_components").join(".spielorders/order.json");
    let bytes = std::fs::read(&record).expect("the Order record can be read");
    std::fs::write(&record, bytes).expect("the Order record is rewritten byte for byte");
}

pub fn remove_index(root: &Path) {
    std::fs::remove_file(root.join(".spielorders/index.json")).expect("the index is removed");
}

pub fn damage_index(root: &Path) {
    std::fs::write(root.join(".spielorders/index.json"), b"{ not json")
        .expect("the index is damaged");
}

pub fn reversion_index(root: &Path) {
    std::fs::write(
        root.join(".spielorders/index.json"),
        b"{\n  \"schema_version\": 999,\n  \"orders\": [],\n  \"vendors\": []\n}\n",
    )
    .expect("the index declares another version");
}

/// [`populated_package`] with a leftover staging artifact beside a valid destination.
///
/// **The one condition whose only finding is a warning.** An interrupted managed write
/// leaves a stage beside its destination; where the destination is valid, that is cleanup
/// rather than damage — so the Package is valid, exits `0`, and still reports the finding
/// and the repair it offers. Written directly because no core capability leaves one
/// behind on purpose: every replacement that succeeds removes its own stage, and one that
/// fails is the failure being reported instead.
///
/// The stage is beside `vendors.json` rather than beside the derived index, because the
/// index's validity is deliberately not this finding's to judge — a stage beside it
/// carries no destination verdict at all.
pub fn package_with_a_staging_artifact(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = populated_package(label);
    std::fs::write(
        root.join(".spielorders/.vendors.json.spielorders-stage"),
        b"{\n  \"schema_version\": 1,\n  \"vendors\": []\n}\n",
    )
    .expect("the leftover staging artifact is written");
    (tree, root)
}

/// [`populated_package`] with one Order whose calculated Item Total is not finite.
///
/// **No core capability can produce this**, because every Order mutation validates the
/// assembled record and refuses one whose totals do not add up. The condition is written
/// into the record the core produced — one Item's two numbers and nothing else — because
/// it is the only route to a non-finite total: a stored non-finite number never loads at
/// all, so two finite numbers whose product overflows is what is left.
///
/// Returns the Package Root and the Order Directory whose total overflows.
pub fn package_with_an_overflowing_total(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let overflowing = order_directory(&root, "Laser_components");
    let record = overflowing.join(".spielorders/order.json");
    let text = std::fs::read_to_string(&record).expect("the fixture Order record can be read");
    let rewritten = text.replace("\"quantity\": 2.0,", "\"quantity\": 1e308,");
    assert_ne!(text, rewritten, "the Quantity member was found");
    std::fs::write(&record, rewritten).expect("the rewritten Order record is written");
    (tree, root, overflowing)
}

/// Every byte under a Package Root, in one deterministic order.
///
/// What "the Package is byte-identical" is asserted against: a dry run must leave no
/// managed file rewritten and no staging artifact behind, and a staging file is exactly
/// the effect a listing of the Root's own entries would miss.
pub fn managed_snapshot(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut found = Vec::new();
    collect_snapshot(root, root, &mut found);
    found.sort();
    found
}

fn collect_snapshot(root: &Path, directory: &Path, found: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
    for name in entry_names(directory) {
        let path = directory.join(name);
        let relative = path
            .strip_prefix(root)
            .expect("every entry is under the Root")
            .to_path_buf();
        if path.is_dir() {
            // The directory itself, so one that appeared or vanished is a difference even
            // when it is empty.
            found.push((relative, None));
            collect_snapshot(root, &path, found);
        } else {
            found.push((
                relative,
                Some(std::fs::read(&path).expect("the file is readable")),
            ));
        }
    }
}

/// When a path was last written, or nothing where there is no such path.
///
/// The half of "wrote nothing" a byte comparison cannot make: a file rewritten with the
/// bytes it already held is byte-identical and was still written.
pub fn modification_time(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::symlink_metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}

/// What one run of the `spielorders` executable did.
pub struct Outcome {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Outcome {
    pub fn stdout_text(&self) -> &str {
        std::str::from_utf8(&self.stdout).expect("standard output is UTF-8")
    }

    pub fn stderr_text(&self) -> &str {
        std::str::from_utf8(&self.stderr).expect("standard error is UTF-8")
    }
}

/// The one response document a `--json` run wrote to standard output.
///
/// Every claim about the envelope goes through here, so "exactly one UTF-8 JSON document
/// and nothing else" is checked by every test that reads a response rather than only by
/// the test that is about it. A second document, a warning line, or a trailing log would
/// fail the parse, which is the point.
pub fn response(outcome: &Outcome) -> serde_json::Value {
    assert_eq!(
        outcome.stderr,
        b"",
        "a `--json` run writes nothing to standard error: {}",
        outcome.stderr_text()
    );
    let text = std::str::from_utf8(&outcome.stdout).expect("the response document is UTF-8");
    let mut documents = serde_json::Deserializer::from_str(text).into_iter::<serde_json::Value>();
    let document = documents
        .next()
        .expect("standard output carries a response document")
        .expect("the response document parses as JSON");
    // Read before asking for a second document: the stream deserializer skips the
    // whitespace between documents while looking for one, which would move the offset
    // past the terminating newline this is here to check.
    let after_document = documents.byte_offset();
    assert!(
        documents.next().is_none(),
        "standard output carries exactly one document: {text}"
    );
    assert_eq!(
        &text[after_document..],
        "\n",
        "the one document is newline-terminated and nothing follows it"
    );
    document
}

/// A plain directory standing inside a Package, which is not itself a SpielOrders Root.
///
/// The Package *above* it is what makes it useful: `--root` names the Root exactly and
/// consults no ancestor, so a command pointed here must fail rather than find the
/// Package one level up. Without the Package above, "found nothing" and "searched
/// nothing" are the same observation.
pub fn directory_that_is_not_a_root(root: &Path) -> PathBuf {
    let inside = root.join("not-a-root");
    std::fs::create_dir(&inside).expect("a plain directory inside the Package is created");
    inside
}

/// A Package whose conventional README exists but is not a readable document.
///
/// A directory standing where the document should be. This is the one warning this
/// command can produce: the conventional path is still the answer, so it is a success
/// carrying a diagnostic rather than a failure.
pub fn package_with_unreadable_readme(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = initialized_package(label);
    std::fs::remove_file(root.join("README.md")).expect("the plain README is removed");
    std::fs::create_dir(root.join("README.md")).expect("a directory takes its place");
    (tree, root)
}

/// A SpielOrders Root that cannot be read past.
///
/// `.spielorders` is present, so the directory *is* a managed Root rather than an
/// unmanaged one — but it is a plain file, so the required child observations under it
/// fail. That is deliberately a shape rather than a permission: `chmod 000` does not
/// stop a suite running as root, and this does.
///
/// The distinction it buys is the one that matters for `context`: the Root resolved and
/// the Package could not be read past it, which is a failure that still has a root to
/// name.
pub fn package_root_that_cannot_be_read(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    std::fs::write(tree.path().join(".spielorders"), b"not a directory")
        .expect("a plain file stands where the metadata directory belongs");
    let root = std::fs::canonicalize(tree.path()).expect("the test root resolves");
    (tree, root)
}

/// The stable stand-in for this run's Package Root in a golden fixture.
///
/// A response carries absolute paths, and no two runs of this suite have the same
/// temporary root. Substituting the root is what leaves every other byte of the document
/// under byte-for-byte comparison; substituting anything more would start hiding the
/// contract from its own regression evidence.
pub const PACKAGE_ROOT: &str = "<PACKAGE_ROOT>";

/// The golden fixture directory for one response contract.
///
/// `contracts/` is the one canonical contract root, and CLI response fixtures live at
/// `contracts/cli/<contract path>/v<contract_version>/` — the path the registry itself
/// derives from the contract identity. Resolved at run time rather than with
/// `include_str!` because the registry is what names it: that is what lets
/// `response_contract.rs` walk every registered contract and fail for a command whose
/// fixtures nobody remembered to add. See `docs/development/traceability.md`.
pub fn fixture_directory(contract: spielorders_cli::contract::ResponseContract) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/cli")
        .join(contract.fixture_directory())
}

/// The stable stand-in for an identity a run minted in the child process.
///
/// A creation's Vendor ID is generated at the commit, so no fixture can carry the one a
/// run produced. Substituting it is what leaves every other byte of the document under
/// byte-for-byte comparison; the substitution asserts the shape of what it replaces, so
/// the fixture still records that a UUIDv4 stood there. See
/// `docs/development/traceability.md`, whose placeholder set this is a member of.
pub const VENDOR_ID: &str = "<VENDOR_ID>";

/// This run's response document, with its Package Root replaced by [`PACKAGE_ROOT`].
pub fn normalized(outcome: &Outcome, root: &Path) -> String {
    let text = std::str::from_utf8(&outcome.stdout).expect("the response document is UTF-8");
    text.replace(
        root.to_str().expect("the test Package root is UTF-8"),
        PACKAGE_ROOT,
    )
}

/// Asserts `value` is a lowercase hyphenated version-4 UUID, and returns it.
///
/// The shape check a substitution owes: a fixture that merely replaced whatever the run
/// happened to emit would record nothing about it, so what the placeholder stands for is
/// pinned here instead.
pub fn minted_uuid_v4(value: &str) -> &str {
    let groups: Vec<&str> = value.split('-').collect();
    assert_eq!(
        groups.iter().map(|group| group.len()).collect::<Vec<_>>(),
        vec![8, 4, 4, 4, 12],
        "{value} is a hyphenated UUID"
    );
    assert!(
        value
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-'),
        "{value} is lowercase hexadecimal and hyphens"
    );
    assert!(
        !value.chars().any(|character| character.is_uppercase()),
        "{value} is lowercase"
    );
    assert!(
        groups[2].starts_with('4'),
        "{value} declares UUID version 4"
    );
    assert!(
        groups[3].starts_with(['8', '9', 'a', 'b']),
        "{value} declares the RFC 4122 variant"
    );
    value
}

/// Compares one response against its golden fixture, byte for byte.
///
/// The golden fixtures are the response contract's whole regression evidence, so a diff
/// here is a contract change and the ticket that causes it says which: a version bump,
/// or a formatting change that advances nothing. The failure prints the actual document
/// so that an intended change can be applied by reading it rather than by guessing.
pub fn assert_matches_golden(
    contract: spielorders_cli::contract::ResponseContract,
    cell: &str,
    outcome: &Outcome,
    root: &Path,
) {
    assert_golden(contract, cell, normalized(outcome, root));
}

/// The same, for a response carrying a Vendor ID this run minted.
///
/// The identity is read out of the response and then replaced, which is only meaningful
/// because [`minted_uuid_v4`] asserts its shape first: the fixture records that a UUIDv4
/// stood there, and every other byte is still compared exactly.
pub fn assert_matches_golden_with_minted_vendor_id(
    contract: spielorders_cli::contract::ResponseContract,
    cell: &str,
    outcome: &Outcome,
    root: &Path,
    minted: &str,
) {
    assert_golden(
        contract,
        cell,
        normalized(outcome, root).replace(minted_uuid_v4(minted), VENDOR_ID),
    );
}

pub fn assert_golden(
    contract: spielorders_cli::contract::ResponseContract,
    cell: &str,
    actual: String,
) {
    let path = fixture_directory(contract).join(format!("{cell}.json"));
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "the golden fixture {} could not be read ({error}). The response was:\n{actual}",
            path.display()
        )
    });
    assert_eq!(
        actual,
        expected,
        "the {} `{cell}` response no longer matches {}. If the change is intended, it is \
         either a `contract_version` bump or a formatting change, and the ticket causing \
         it says which.",
        contract.name(),
        path.display()
    );
}

/// One run of the real `spielorders` executable.
///
/// Every claim this suite makes about exit status, stream discipline, and the response
/// contract is asserted from a real process, because those are process-level facts and
/// an internal return value cannot stand in for one.
pub struct Invocation {
    arguments: Vec<OsString>,
    working_directory: Option<PathBuf>,
    environment: Vec<(String, OsString)>,
}

impl Invocation {
    pub fn new(arguments: &[&str]) -> Self {
        Self {
            arguments: arguments.iter().map(OsString::from).collect(),
            working_directory: None,
            environment: Vec::new(),
        }
    }

    pub fn argument(mut self, argument: impl AsRef<OsStr>) -> Self {
        self.arguments.push(argument.as_ref().to_owned());
        self
    }

    /// Targets `root` explicitly. Separate from [`Self::in_directory`] so a test that
    /// is about the default target says so by not calling this.
    pub fn rooted_at(self, root: &Path) -> Self {
        self.argument("--root").argument(root)
    }

    pub fn in_directory(mut self, directory: &Path) -> Self {
        self.working_directory = Some(directory.to_path_buf());
        self
    }

    pub fn with_environment(mut self, key: &str, value: impl AsRef<OsStr>) -> Self {
        self.environment
            .push((key.to_string(), value.as_ref().to_owned()));
        self
    }

    pub fn run(self) -> Outcome {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_spielorders"));
        command.args(&self.arguments);
        // The developer's own shell must not decide what these tests observe. Both
        // streams are pipes here, so color is off by default; these are the variables
        // that could turn it back on or off behind a test's back.
        for inherited in ["NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE"] {
            command.env_remove(inherited);
        }
        for (key, value) in &self.environment {
            command.env(key, value);
        }
        if let Some(directory) = &self.working_directory {
            command.current_dir(directory);
        }
        let output = command.output().expect("the spielorders executable runs");
        Outcome {
            status: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        }
    }
}

/// A Package with one unmanaged candidate directory under its Orders Directory.
///
/// The candidate holds a user-authored `README.md` and a Supporting Material subtree,
/// which is what makes "adopted with both byte-identical and nothing else created,
/// renamed, or removed" a claim about a real directory. Its name embeds an impossible
/// calendar date on purpose: `2009_04_31` is the shape a legacy archive actually has, and
/// adoption must take the supplied Date Ordered and leave the name alone.
///
/// Returns the Package Root and the candidate's directory.
pub fn package_with_an_adoption_candidate(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let candidate = unmanaged_candidate(&root, "2009_04_31_Lcom_Fiber");
    std::fs::write(
        candidate.join("README.md"),
        b"Ordered by telephone. Invoice in the quotes folder.\n",
    )
    .expect("the candidate README is written");
    let material = candidate.join("Quotes");
    std::fs::create_dir(&material).expect("the candidate's Supporting Material is created");
    std::fs::write(material.join("quote.txt"), b"Fiber, 3 metres.\n")
        .expect("the Supporting Material is written");
    (tree, root, candidate)
}

/// One plain directory standing in a Package's Orders Directory, under management by
/// nothing.
///
/// The Orders Directory is found through the Package's own configured path rather than
/// spelled here, so a fixture does not decide where a Package keeps its Orders.
pub fn unmanaged_candidate(root: &Path, name: &str) -> PathBuf {
    let candidate = orders_directory(root).join(name);
    std::fs::create_dir(&candidate).expect("the candidate directory is created");
    candidate
}

/// A Package's configured Removed Orders Directory, as the core reports it.
///
/// Asked of the core rather than spelled here, so a test about the disposition's
/// destination is about the place the Package actually configures. It is created lazily,
/// so this names a path that may not be there.
pub fn removed_orders_directory(root: &Path) -> PathBuf {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::canonical::{CanonicalPackageLoad, inspect_canonical_package};

    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(root, &PlatformFileSystem)
    else {
        panic!("the fixture Package is a current candidate");
    };
    package
        .context()
        .removed_orders_directory()
        .expect("a current Package names a Removed Orders Directory")
        .absolute_path()
        .to_path_buf()
}

/// A Package's configured Orders Directory, as the core reports it.
pub fn orders_directory(root: &Path) -> PathBuf {
    use spielorders_cli::effects::PlatformFileSystem;
    use spielorders_core::canonical::{CanonicalPackageLoad, inspect_canonical_package};

    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(root, &PlatformFileSystem)
    else {
        panic!("the fixture Package is a current candidate");
    };
    package
        .context()
        .orders_directory()
        .expect("a current Package has an Orders Directory")
        .absolute_path()
        .to_path_buf()
}

/// The assignments an `adopt-order` command line has to supply, as a caller writes them.
///
/// The three required facts and nothing else. Named here because every adoption test
/// supplies them and none of them is the subject of any one test.
pub const ADOPTED_FACTS: [&str; 3] = [
    "title=Fiber patch cable",
    "date_ordered=2009-04-30",
    "status=complete",
];

/// The stable stand-in for an Order ID a run minted in the child process.
///
/// A creation's and an adoption's Order ID is generated at the commit, so no fixture can
/// carry the one a run produced. See `docs/development/traceability.md`, whose closed
/// placeholder set this is a member of.
pub const ORDER_ID: &str = "<ORDER_ID>";

/// The stable stand-in for the local date a run read from the clock.
///
/// `add-order` stamps today's local Date Ordered when the command line assigns none, and
/// that date reaches the response twice: as the Order's own fact, and inside the projected
/// Order Directory basename. Both are replaced, because they are one value in two
/// spellings. See `docs/development/traceability.md`.
pub const TODAY: &str = "<TODAY>";

/// This run's response document, with everything no fixture can carry replaced.
///
/// The Package Root always; the minted Order ID where the run committed one; and today's
/// date where the run read the clock. Every other byte is left for byte-for-byte
/// comparison.
///
/// **Each substitution is of a value this test knows independently**, which is what it
/// asserts: today's date is read from the executable's own calendar effect and the Order ID
/// is shape-checked before either is replaced, so a run that stamped another date or
/// emitted something that is not a UUIDv4 fails the comparison rather than being
/// normalized into agreement.
pub fn normalized_with_substitutions(
    outcome: &Outcome,
    root: &Path,
    minted: Option<&str>,
) -> String {
    use spielorders_core::ports::LocalCalendar;

    let mut text = normalized(outcome, root);
    if let Some(minted) = minted {
        text = text.replace(minted_uuid_v4(minted), ORDER_ID);
    }
    let today = spielorders_cli::effects::PlatformCalendar.today();
    for spelling in [
        format!("{:04}-{:02}-{:02}", today.year, today.month, today.day),
        format!("{:04}_{:02}_{:02}", today.year, today.month, today.day),
    ] {
        text = text.replace(&spelling, TODAY);
    }
    text
}

/// Compares one response against its golden fixture, substituting what no fixture can
/// carry.
///
/// `minted` is the identity a run generated, for the two commands that mint one; every
/// response goes through the same clock substitution, because an `order edit` that enters
/// `ordered` with no date reads today's local date and no fixture can carry that either.
/// See [`normalized_with_substitutions`] for the whole closed set.
pub fn assert_matches_golden_with_substitutions(
    contract: spielorders_cli::contract::ResponseContract,
    cell: &str,
    outcome: &Outcome,
    root: &Path,
    minted: Option<&str>,
) {
    assert_golden(
        contract,
        cell,
        normalized_with_substitutions(outcome, root, minted),
    );
}

/// [`populated_package`] with one Order referencing a Vendor this Package has none of.
///
/// **No core capability can produce this**, because every Order mutation refuses a
/// `vendor_id` naming no Vendor of the Package. The condition is therefore written into
/// the record the core produced — one member's text and nothing else — because it is the
/// Unresolved Vendor Reference the integrity decision makes explicitly repairable.
///
/// Returns the Package Root and the Order Directory whose reference no longer resolves.
pub fn package_with_an_unresolved_vendor_reference(
    label: &str,
) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let damaged = order_directory(&root, "Mirror_mounts");
    let record = damaged.join(".spielorders/order.json");
    let text = std::fs::read_to_string(&record).expect("the fixture Order record can be read");
    let rewritten = text.replace(HIDDEN_VENDOR_ID, MISSING_VENDOR_ID);
    assert_ne!(text, rewritten, "the Vendor Reference was found");
    std::fs::write(&record, rewritten).expect("the rewritten Order record is written");
    (tree, root, damaged)
}

/// A Vendor ID no Vendor of the fixture Package carries.
pub const MISSING_VENDOR_ID: &str = "4d2f6a18-9c05-4b3e-8f61-2a7d0e5c8b94";

/// [`populated_package`] with two Order Directories carrying one Order ID.
///
/// **No core capability can produce this either**: every creation and adoption checks a
/// freshly minted identity against the Package before writing it. Every directory sharing
/// a duplicate Order ID is a Damaged Order, and explicit repair requires the user to
/// designate which one keeps the identity — so the other adopts a new one.
///
/// Returns the Package Root and the Order Directory whose identity was overwritten, which
/// is the side a test then gives a fresh identity to.
pub fn package_with_a_duplicate_order_id(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let conflicting = order_directory(&root, "Mirror_mounts");
    let record = conflicting.join(".spielorders/order.json");
    let text = std::fs::read_to_string(&record).expect("the fixture Order record can be read");
    let rewritten = text.replace(OLDEST_ORDER_ID, NEWEST_ORDER_ID);
    assert_ne!(text, rewritten, "the Order ID was found");
    std::fs::write(&record, rewritten).expect("the rewritten Order record is written");
    (tree, root, conflicting)
}

/// [`populated_package`] with three Order Directories carrying one Order ID.
///
/// **Two participants cannot tell a count from a count plus one**: where two directories
/// conflict, a sentence saying two and a sentence saying three are equally consistent with
/// what a reader sees, so the claim needs a second conflict of a different size.
///
/// Returns the Package Root and the third Order Directory.
pub fn package_with_three_duplicate_order_ids(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    // Created before anything conflicts, because creation checks its minted identity
    // against the Package and a Package with Damaged Orders is not one a mutation reads.
    let third = order_created_in(&root, "Optical table", THIRD_ORDER_ID);
    for (directory, existing) in [
        (order_directory(&root, "Mirror_mounts"), OLDEST_ORDER_ID),
        (third.clone(), THIRD_ORDER_ID),
    ] {
        let record = directory.join(".spielorders/order.json");
        let text = std::fs::read_to_string(&record).expect("the fixture Order record can be read");
        let rewritten = text.replace(existing, NEWEST_ORDER_ID);
        assert_ne!(text, rewritten, "the Order ID was found");
        std::fs::write(&record, rewritten).expect("the rewritten Order record is written");
    }
    (tree, root, third)
}

/// [`populated_package`] with a leftover staging artifact beside one Order's record.
///
/// An interrupted managed write leaves exactly this: a durable stage beside the
/// destination it was to become. No capability produces one on purpose — every replacement
/// that succeeds removes its own stage, and one that fails is the failure being reported
/// instead — so the fixture writes it, carrying a complete valid record with one fact
/// changed, which is what makes promoting it observable.
///
/// Returns the Package Root and the Order Directory the stage sits in.
pub fn package_with_an_order_staging_artifact(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = populated_package(label);
    let directory = order_directory(&root, "Mirror_mounts");
    let record = directory.join(".spielorders/order.json");
    let text = std::fs::read_to_string(&record).expect("the fixture Order record can be read");
    let candidate = text.replace(
        "\"title\": \"Mirror mounts\"",
        "\"title\": \"Mirror mounts II\"",
    );
    assert_ne!(text, candidate, "the Title member was found");
    std::fs::write(
        directory.join(".spielorders/.order.json.spielorders-stage"),
        candidate,
    )
    .expect("the leftover staging artifact is written");
    (tree, root, directory)
}

/// [`populated_package`] with a leftover staging artifact beside its Vendor aggregate, and
/// the derived index refresh blocked.
///
/// The two conditions together are what a committed-with-warnings repair needs: a
/// Package-level case to apply, and a real filesystem refusal of the index rebuild that
/// follows the canonical write.
pub fn package_with_a_package_case_and_a_blocked_index_refresh(
    label: &str,
) -> (TemporaryTree, PathBuf) {
    let (tree, root) = package_with_a_staging_artifact(label);
    block_index_refresh(&root);
    (tree, root)
}
