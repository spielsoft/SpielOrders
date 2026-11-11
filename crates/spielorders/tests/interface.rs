//! The interface as a Rust caller uses it: the exported functions through their safe
//! signatures, against disposable Packages on the real filesystem.
//!
//! Decision sources: Effect and adapter boundaries, Interface rules; Define the Rust CLI
//! capability contract; Prototype the SpielOrders desktop experience; Define integrity,
//! validation, and repair boundaries; Define legacy Order adoption and metadata
//! bootstrapping.

use spielorders::*;
use std::{
    ffi::CString,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Directory(PathBuf);

impl Directory {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "spielorders-interface-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn c(text: &str) -> CString {
    CString::new(text).unwrap()
}

fn c_path(path: &Path) -> CString {
    CString::new(path.as_os_str().as_bytes()).unwrap()
}

fn string(text: Text<'_>) -> String {
    text.to_str().unwrap().to_owned()
}

fn path(path: PathBytes<'_>) -> PathBuf {
    path.as_path().to_path_buf()
}

/// The codes of every entry, by name.
fn codes(entries: &Option<Box<SpielordersEntries>>) -> Vec<String> {
    (0..spielorders_entries_count(entries.as_deref()))
        .map(|i| {
            string(spielorders_constant_name(spielorders_entries_code(
                entries.as_deref(),
                i,
            )))
        })
        .collect()
}

/// Each entry as (code, field, row) names.
fn located(entries: &Option<Box<SpielordersEntries>>) -> Vec<(String, String, i64)> {
    let entries = entries.as_deref();
    (0..spielorders_entries_count(entries))
        .map(|i| {
            (
                string(spielorders_constant_name(spielorders_entries_code(
                    entries, i,
                ))),
                string(spielorders_constant_name(spielorders_entries_field(
                    entries, i,
                ))),
                spielorders_entries_row(entries, i),
            )
        })
        .collect()
}

fn commit(plan: &SpielordersPlan) -> Box<SpielordersReport> {
    let mut report = None;
    let mut outcome = None;
    let result = spielorders_plan_commit(plan, &mut report, Some(&mut outcome));
    assert_eq!(result, SPIELORDERS_RESULT_OK, "{:?}", located(&outcome));
    report.unwrap()
}

fn initialized(label: &str) -> Directory {
    let directory = Directory::new(label);
    let mut plan = None;
    let root = c_path(&directory.0);
    assert_eq!(
        spielorders_plan_initialize(
            PathBytes::new(&root),
            OptionalPathBytes::null(),
            OptionalPathBytes::null(),
            0,
            &mut plan,
            None,
        ),
        SPIELORDERS_RESULT_OK
    );
    commit(&plan.unwrap());
    directory
}

fn open(root: &Path) -> Box<SpielordersPackage> {
    let mut package = None;
    let root = c_path(root);
    let mut outcome = None;
    let result =
        spielorders_package_open(PathBytes::new(&root), 0, &mut package, Some(&mut outcome));
    assert_eq!(result, SPIELORDERS_RESULT_OK, "{:?}", located(&outcome));
    package.unwrap()
}

#[test]
fn agent_setup_provisions_only_missing_guidance_and_preserves_customizations() {
    let directory = initialized("agent-setup");
    let root = &directory.0;
    let skill = root.join(".agents/skills/spielorders");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "custom skill\n").unwrap();
    fs::write(root.join("AGENTS.md"), "owner instructions\n").unwrap();
    let index = fs::read(root.join(".spielorders/index.json")).unwrap();
    let package = open(root);
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    assert!(
        !skill.join("references").exists(),
        "planning writes nothing"
    );
    assert_eq!(
        spielorders_report_kind(&commit(&plan.unwrap())),
        SPIELORDERS_REPORT_COMMITTED
    );
    assert!(skill.join("references/packages-and-adoption.md").is_file());
    assert_eq!(
        fs::read_to_string(skill.join("SKILL.md")).unwrap(),
        "custom skill\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "owner instructions\n"
    );
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index
    );
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    assert_eq!(
        spielorders_report_kind(&commit(&plan.unwrap())),
        SPIELORDERS_REPORT_UNCHANGED
    );
    assert!(
        string(spielorders_library_agent_setup_prompt())
            .contains(".agents/skills/spielorders/SKILL.md")
    );
}

#[test]
fn agent_setup_refuses_stale_configuration_and_concurrent_destinations_before_writing() {
    let directory = initialized("agent-setup-stale");
    let root = &directory.0;
    let package = open(root);
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    let plan = plan.unwrap();
    fs::write(
        root.join(".spielorders/config.toml"),
        "removed_path = \"Archived\"\n",
    )
    .unwrap();
    let mut report = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_plan_commit(&plan, &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(codes(&outcome).contains(&"preconditions_changed".to_owned()));
    assert!(!root.join(".agents").exists());

    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK,
        "planning reads fresh configuration"
    );
    fs::create_dir(root.join(".agents")).unwrap();
    fs::write(root.join(".agents/owner.txt"), "concurrent work").unwrap();
    assert_eq!(
        spielorders_plan_commit(&plan.unwrap(), &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(
        fs::read_to_string(root.join(".agents/owner.txt")).unwrap(),
        "concurrent work"
    );
    assert!(!root.join(".agents/skills").exists());
}

#[test]
fn agent_setup_cancels_and_rejects_unsuitable_components_without_touching_root_instructions() {
    use std::os::unix::fs::symlink;
    let directory = initialized("agent-setup-paths");
    let root = &directory.0;
    let outside = Directory::new("agent-setup-outside");
    fs::write(outside.0.join("instructions"), "private owner instructions").unwrap();
    symlink(outside.0.join("instructions"), root.join("AGENTS.md")).unwrap();
    let package = open(root);
    let token = spielorders_cancel_new();
    spielorders_cancel_request(token);
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, token, &mut plan, None),
        SPIELORDERS_RESULT_CANCELLED
    );
    spielorders_cancel_release(token);
    assert!(!root.join(".agents").exists());
    fs::write(root.join(".agents"), "not a directory").unwrap();
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, None),
        SPIELORDERS_RESULT_REFUSED
    );
    fs::remove_file(root.join(".agents")).unwrap();
    symlink(&outside.0, root.join(".agents")).unwrap();
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, None),
        SPIELORDERS_RESULT_REFUSED
    );
    fs::remove_file(root.join(".agents")).unwrap();
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    commit(&plan.unwrap());
    assert_eq!(
        fs::read_to_string(outside.0.join("instructions")).unwrap(),
        "private owner instructions"
    );
    assert!(
        fs::symlink_metadata(root.join("AGENTS.md"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn agent_setup_refuses_guidance_overlapping_configured_managed_directories() {
    for (label, config) in [
        ("agent-overlap-removed", "removed_path = \".agents\"\n"),
        ("agent-overlap-orders", "orders_path = \".agents\"\n"),
    ] {
        let directory = initialized(label);
        let root = &directory.0;
        fs::write(root.join(".spielorders/config.toml"), config).unwrap();
        if config.starts_with("orders") {
            fs::create_dir(root.join(".agents")).unwrap();
        }
        let package = open(root);
        let mut plan = None;
        assert_eq!(
            spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, None),
            SPIELORDERS_RESULT_REFUSED
        );
        assert!(!root.join(".agents/skills").exists());
    }
}

#[test]
fn agent_setup_refuses_an_orders_directory_linked_to_the_guidance() {
    use std::os::unix::fs::symlink;
    let directory = initialized("agent-alias-plan");
    let root = &directory.0;
    fs::create_dir(root.join(".agents")).unwrap();
    symlink(root.join(".agents"), root.join("Linked")).unwrap();
    fs::write(
        root.join(".spielorders/config.toml"),
        "orders_path = \"Linked\"\n",
    )
    .unwrap();
    let package = open(root);
    let mut plan = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(codes(&outcome).contains(&"destination_collision".to_owned()));
    assert!(!root.join(".agents/skills").exists());
}

#[test]
fn agent_setup_rolls_back_when_the_orders_directory_becomes_a_link_to_the_guidance() {
    use std::os::unix::fs::symlink;
    let directory = initialized("agent-alias-commit");
    let root = &directory.0;
    fs::create_dir(root.join("Linked")).unwrap();
    fs::write(
        root.join(".spielorders/config.toml"),
        "orders_path = \"Linked\"\n",
    )
    .unwrap();
    let package = open(root);
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_provision_agent_setup(&package, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    fs::remove_dir(root.join("Linked")).unwrap();
    symlink(root.join(".agents"), root.join("Linked")).unwrap();
    let mut report = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_plan_commit(&plan.unwrap(), &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(
        !root.join(".agents").exists(),
        "the created directory is removed"
    );
}

fn set(draft: &mut SpielordersDraft, field: SpielordersConstant, value: &str) {
    let value = c(value);
    assert_eq!(
        spielorders_draft_set(draft, field, Text::new(&value), None),
        SPIELORDERS_RESULT_OK
    );
}

fn set_row(draft: &mut SpielordersDraft, row: i64, field: SpielordersConstant, value: &str) {
    let value = c(value);
    assert_eq!(
        spielorders_draft_row_set(draft, row, field, Text::new(&value), None),
        SPIELORDERS_RESULT_OK
    );
}

/// The plan of `draft`, or each entry it was refused with as (code, field, row).
fn plan_draft(
    draft: &SpielordersDraft,
) -> Result<Box<SpielordersPlan>, Vec<(String, String, i64)>> {
    let mut plan = None;
    let mut outcome = None;
    match spielorders_draft_plan(draft, 0, &mut plan, Some(&mut outcome)) {
        SPIELORDERS_RESULT_OK => Ok(plan.unwrap()),
        _ => Err(located(&outcome)),
    }
}

/// Creates an Order titled `title` with `items` (description, quantity, unit price) and
/// returns its directory.
fn created_order(root: &Path, title: &str, items: &[(&str, &str, &str)]) -> PathBuf {
    let package = open(root);
    let mut draft = None;
    assert_eq!(
        spielorders_package_new_order_draft(&package, &mut draft, None),
        SPIELORDERS_RESULT_OK
    );
    let mut draft = draft.unwrap();
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, title);
    for (row, (description, quantity, unit_price)) in items.iter().enumerate() {
        let row = i64::try_from(row).unwrap();
        assert_eq!(
            spielorders_draft_row_insert(&mut draft, row, None),
            SPIELORDERS_RESULT_OK
        );
        set_row(
            &mut draft,
            row,
            SPIELORDERS_ITEM_FIELD_DESCRIPTION,
            description,
        );
        set_row(&mut draft, row, SPIELORDERS_ITEM_FIELD_QUANTITY, quantity);
        set_row(
            &mut draft,
            row,
            SPIELORDERS_ITEM_FIELD_UNIT_PRICE,
            unit_price,
        );
    }
    let report = commit(&plan_draft(&draft).unwrap());
    path(spielorders_report_final_path(&report))
}

fn read_order(package: &SpielordersPackage, directory: &Path) -> Box<SpielordersOrder> {
    let directory = c_path(directory);
    let mut order = None;
    let mut outcome = None;
    let result = spielorders_package_read_order(
        package,
        PathBytes::new(&directory),
        0,
        &mut order,
        Some(&mut outcome),
    );
    assert_eq!(result, SPIELORDERS_RESULT_OK, "{:?}", located(&outcome));
    order.unwrap()
}

fn order_draft(order: &SpielordersOrder) -> Box<SpielordersDraft> {
    let mut draft = None;
    assert_eq!(
        spielorders_order_draft(order, &mut draft),
        SPIELORDERS_RESULT_OK
    );
    draft.unwrap()
}

fn titles(list: &SpielordersList) -> Vec<String> {
    (0..spielorders_list_count(list))
        .map(|i| {
            string(spielorders_list_field(
                list,
                i,
                SPIELORDERS_ORDER_FIELD_TITLE,
            ))
        })
        .collect()
}

fn orders(
    package: &SpielordersPackage,
    query: Option<&str>,
    flags: u32,
    sort: SpielordersConstant,
    descending: i32,
) -> Box<SpielordersList> {
    let query = query.map(c);
    let mut list = None;
    let result = spielorders_package_orders(
        package,
        query
            .as_deref()
            .map_or(OptionalText::null(), OptionalText::new),
        flags,
        sort,
        descending,
        0,
        &mut list,
        None,
    );
    assert_eq!(result, SPIELORDERS_RESULT_OK);
    list.unwrap()
}

#[test]
fn every_constant_has_a_name_that_reads_back_as_that_constant() {
    for (family, constant) in [
        (
            SPIELORDERS_FAMILY_FAILURE,
            SPIELORDERS_FAILURE_DAMAGED_PACKAGE,
        ),
        (
            SPIELORDERS_FAMILY_FINDING,
            SPIELORDERS_FINDING_ORDER_ID_DUPLICATE,
        ),
        (
            SPIELORDERS_FAMILY_ORDER_STATUS,
            SPIELORDERS_ORDER_STATUS_ORDERED,
        ),
        (
            SPIELORDERS_FAMILY_ORDER_FIELD,
            SPIELORDERS_ORDER_FIELD_TITLE,
        ),
        (
            SPIELORDERS_FAMILY_ITEM_FIELD,
            SPIELORDERS_ITEM_FIELD_DESCRIPTION,
        ),
    ] {
        let name = spielorders_constant_name(constant);
        assert_eq!(spielorders_constant_from_name(family, name), constant);
    }
    assert_eq!(
        string(spielorders_constant_name(
            SPIELORDERS_FAILURE_DAMAGED_PACKAGE
        )),
        "damaged_package"
    );
    assert_eq!(
        string(spielorders_constant_name(SpielordersConstant(-7))),
        ""
    );
    assert_eq!(
        spielorders_constant_from_name(SPIELORDERS_FAMILY_ITEM_FIELD, Text::new(c"title")),
        SpielordersConstant(-1),
        "a name of another family is no member of this one"
    );
}

#[test]
fn opening_refuses_a_folder_that_is_not_a_package_and_a_root_that_is_a_link() {
    let folder = Directory::new("unmanaged");
    let root = c_path(&folder.0);
    let mut package = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_package_open(PathBytes::new(&root), 0, &mut package, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(package.is_none());
    assert_eq!(codes(&outcome), ["unmanaged_package"]);

    let real = initialized("linked");
    let link = folder.0.join("link");
    std::os::unix::fs::symlink(&real.0, &link).unwrap();
    let link = c_path(&link);
    let mut outcome = None;
    assert_eq!(
        spielorders_package_open(PathBytes::new(&link), 0, &mut package, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(codes(&outcome), ["root_is_a_link"]);
}

#[test]
fn an_order_list_filters_sorts_and_shows_damaged_orders_only_unfiltered() {
    let root = initialized("lists");
    created_order(&root.0, "Laser components", &[("Lens", "2", "10")]);
    created_order(&root.0, "Mirror mounts", &[("Mount", "1", "84.5")]);
    let damaged = created_order(&root.0, "Breadboard", &[]);
    fs::write(damaged.join(".spielorders/order.json"), "{").unwrap();
    let package = open(&root.0);

    let all = orders(&package, None, 0, SPIELORDERS_ORDER_FIELD_TOTAL, 1);
    assert_eq!(titles(&all)[..2], ["Mirror mounts", "Laser components"]);
    assert_eq!(spielorders_list_count(&all), 3);
    assert_eq!(spielorders_list_damaged(&all, 2), 1);
    assert_eq!(
        string(spielorders_list_field(
            &all,
            2,
            SPIELORDERS_ORDER_FIELD_DAMAGE
        )),
        "managed_artifact_invalid_syntax"
    );
    assert_eq!(
        string(spielorders_list_field(
            &all,
            0,
            SPIELORDERS_ORDER_FIELD_TOTAL
        )),
        "84.5"
    );

    assert_eq!(
        titles(&orders(
            &package,
            Some("laser"),
            0,
            SpielordersConstant(-1),
            0
        )),
        ["Laser components"]
    );
    assert!(
        titles(&orders(
            &package,
            Some("laser"),
            SPIELORDERS_LIST_CASE_SENSITIVE,
            SpielordersConstant(-1),
            0
        ))
        .is_empty()
    );

    let mut list = None;
    assert_eq!(
        spielorders_package_orders(
            &package,
            OptionalText::null(),
            0,
            SPIELORDERS_ITEM_FIELD_QUANTITY,
            0,
            0,
            &mut list,
            None
        ),
        SPIELORDERS_RESULT_BAD_ARGUMENT,
        "a sort field of another family"
    );
}

#[test]
fn a_damaged_package_opens_for_diagnosis_only() {
    let root = initialized("damaged-package");
    fs::write(root.0.join(".spielorders/vendors.json"), "[").unwrap();
    let package = open(&root.0);
    assert_eq!(
        spielorders_package_state(&package),
        SPIELORDERS_PACKAGE_STATE_DAMAGED
    );
    assert!(spielorders_entries_count(spielorders_package_findings(&package).as_deref()) > 0);

    let mut list = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_package_orders(
            &package,
            OptionalText::null(),
            0,
            SpielordersConstant(-1),
            0,
            0,
            &mut list,
            Some(&mut outcome)
        ),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(codes(&outcome)[0], "damaged_package");

    let mut diagnosis = None;
    assert_eq!(
        spielorders_package_validate(&package, 0, &mut diagnosis, None),
        SPIELORDERS_RESULT_OK
    );
    assert_eq!(spielorders_diagnosis_is_valid(&diagnosis.unwrap()), 0);
}

#[test]
fn a_read_order_answers_its_facts_rows_and_readme() {
    let root = initialized("read");
    let directory = created_order(&root.0, "Laser components", &[("Lens", "2", "10.5")]);
    let package = open(&root.0);
    let order = read_order(&package, &directory);
    assert_eq!(
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_TITLE
        )),
        "Laser components"
    );
    assert_eq!(
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_STATUS
        )),
        "in_preparation"
    );
    assert_eq!(
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_TOTAL
        )),
        "21"
    );
    assert_eq!(spielorders_order_item_count(&order), 1);
    assert_eq!(
        string(spielorders_order_item_field(
            &order,
            0,
            SPIELORDERS_ITEM_FIELD_QUANTITY
        )),
        "2"
    );
    assert_eq!(
        string(spielorders_order_item_field(
            &order,
            5,
            SPIELORDERS_ITEM_FIELD_QUANTITY
        )),
        ""
    );
    assert_eq!(path(spielorders_order_directory(&order)), directory);
    let readme = spielorders_order_readme(&order).unwrap();
    assert_eq!(
        spielorders_readme_state(&readme),
        SPIELORDERS_README_STATE_PRESENT
    );
}

/// Every calculated total crosses unrounded, as text a caller parses back to exactly the
/// value the library calculated.
///
/// `3 × 0.1` calculates to more than two decimal places, and two Items of `0.004` each
/// round to nothing while their sum rounds to a cent, so a total spelled from rounded
/// parts, or rounded itself, reads back as a different number.
#[test]
fn totals_cross_unrounded_and_read_back_as_the_calculated_value() {
    let root = initialized("unrounded");
    let directory = created_order(
        &root.0,
        "Fasteners",
        &[
            ("Shim", "3", "0.1"),
            ("Washer", "1", "0.004"),
            ("Nut", "1", "0.004"),
        ],
    );
    let package = open(&root.0);
    let order = read_order(&package, &directory);
    let draft = order_draft(&order);
    let list = orders(&package, None, 0, SpielordersConstant(-1), 0);
    let number = |text: Text<'_>| string(text).parse::<f64>().unwrap();
    let items = [3.0 * 0.1, 0.004, 0.004];
    let total = items.iter().fold(0.0, |sum, item| sum + item);

    for (row, item) in (0..).zip(items) {
        assert_eq!(
            number(spielorders_order_item_field(
                &order,
                row,
                SPIELORDERS_ITEM_FIELD_TOTAL
            )),
            item,
            "a read Item"
        );
        assert_eq!(
            number(spielorders_draft_row_field(
                &draft,
                row,
                SPIELORDERS_ITEM_FIELD_TOTAL
            )),
            item,
            "a draft row"
        );
    }
    assert_eq!(
        number(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_TOTAL
        )),
        total,
        "a read Order"
    );
    assert_eq!(number(spielorders_draft_total(&draft)), total, "a draft");
    assert_eq!(
        number(spielorders_draft_field(
            &draft,
            SPIELORDERS_ORDER_FIELD_TOTAL
        )),
        total,
        "a draft's field"
    );
    assert_eq!(
        number(spielorders_list_field(
            &list,
            0,
            SPIELORDERS_ORDER_FIELD_TOTAL
        )),
        total,
        "a listed Order"
    );
}

#[test]
fn a_draft_keeps_what_was_typed_and_applies_the_live_rules() {
    let root = initialized("draft-rules");
    let directory = created_order(&root.0, "Laser components", &[("Lens", "2", "10")]);
    let package = open(&root.0);
    let mut draft = order_draft(&read_order(&package, &directory));

    set(&mut draft, SPIELORDERS_ORDER_FIELD_DATE_ORDERED, "");
    set(&mut draft, SPIELORDERS_ORDER_FIELD_STATUS, "ordered");
    assert_ne!(
        string(spielorders_draft_field(
            &draft,
            SPIELORDERS_ORDER_FIELD_DATE_ORDERED
        )),
        "",
        "entering Ordered takes today"
    );

    set_row(&mut draft, 0, SPIELORDERS_ITEM_FIELD_QUANTITY, "three");
    assert_eq!(
        string(spielorders_draft_row_field(
            &draft,
            0,
            SPIELORDERS_ITEM_FIELD_QUANTITY
        )),
        "three"
    );
    assert_eq!(string(spielorders_draft_total(&draft)), "");
    set_row(&mut draft, 0, SPIELORDERS_ITEM_FIELD_QUANTITY, "3");
    assert_eq!(string(spielorders_draft_total(&draft)), "30");

    let value = c("x");
    assert_eq!(
        spielorders_draft_set(
            &mut draft,
            SPIELORDERS_VENDOR_FIELD_PHONE,
            Text::new(&value),
            None
        ),
        SPIELORDERS_RESULT_BAD_ARGUMENT,
        "a field of another family"
    );
    let invalid = c"\xff";
    assert_eq!(
        spielorders_draft_set(
            &mut draft,
            SPIELORDERS_ORDER_FIELD_TITLE,
            Text::new(invalid),
            None
        ),
        SPIELORDERS_RESULT_BAD_ARGUMENT,
        "text that is not UTF-8"
    );
    let mut outcome = None;
    assert_eq!(
        spielorders_draft_row_remove(&mut draft, 4, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(
        located(&outcome),
        [("invalid_item_position".to_owned(), String::new(), 4)]
    );

    let mut adoption = None;
    let candidate = root.0.join("Orders/Legacy");
    fs::create_dir(&candidate).unwrap();
    let candidate = c_path(&candidate);
    assert_eq!(
        spielorders_package_adoption_draft(
            &package,
            PathBytes::new(&candidate),
            0,
            &mut adoption,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    let mut adoption = adoption.unwrap();
    set(&mut adoption, SPIELORDERS_ORDER_FIELD_STATUS, "ordered");
    assert_eq!(
        string(spielorders_draft_field(
            &adoption,
            SPIELORDERS_ORDER_FIELD_DATE_ORDERED
        )),
        "",
        "adoption never takes today"
    );
}

#[test]
fn a_saved_draft_plans_its_changes_and_commits_them() {
    let root = initialized("save");
    let directory = created_order(
        &root.0,
        "Laser components",
        &[("Lens", "2", "10"), ("Iris", "1", "55")],
    );
    let package = open(&root.0);
    let mut draft = order_draft(&read_order(&package, &directory));
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Laser optics");
    set(&mut draft, SPIELORDERS_ORDER_FIELD_PROJECT, "Telescope");
    assert_eq!(
        spielorders_draft_confirm(&mut draft, SPIELORDERS_ORDER_FIELD_PROJECT, -1, None),
        SPIELORDERS_RESULT_OK
    );
    assert_eq!(
        spielorders_draft_row_move(&mut draft, 1, 0, None),
        SPIELORDERS_RESULT_OK
    );
    assert_eq!(
        spielorders_draft_row_insert(&mut draft, 2, None),
        SPIELORDERS_RESULT_OK
    );
    set_row(&mut draft, 2, SPIELORDERS_ITEM_FIELD_DESCRIPTION, "Post");
    set_row(&mut draft, 2, SPIELORDERS_ITEM_FIELD_QUANTITY, "4");
    set_row(&mut draft, 2, SPIELORDERS_ITEM_FIELD_UNIT_PRICE, "6");

    let plan = plan_draft(&draft).unwrap();
    let changes: Vec<(String, String, i64, String, String, String)> = (0
        ..spielorders_plan_change_count(&plan))
        .map(|i| {
            (
                string(spielorders_constant_name(spielorders_plan_change_kind(
                    &plan, i,
                ))),
                string(spielorders_constant_name(spielorders_plan_change_field(
                    &plan, i,
                ))),
                spielorders_plan_change_row(&plan, i),
                string(spielorders_plan_change_before(&plan, i)),
                string(spielorders_plan_change_after(&plan, i)),
                string(spielorders_constant_name(spielorders_plan_change_reuse(
                    &plan, i,
                ))),
            )
        })
        .collect();
    let owned = |kind: &str, field: &str, row, before: &str, after: &str, reuse: &str| {
        (
            kind.to_owned(),
            field.to_owned(),
            row,
            before.to_owned(),
            after.to_owned(),
            reuse.to_owned(),
        )
    };
    assert_eq!(
        changes,
        [
            owned("fact", "title", -1, "Laser components", "Laser optics", ""),
            owned("fact", "project", -1, "", "Telescope", "new"),
            owned("row_moved", "", 0, "1", "0", ""),
            owned("row_moved", "", 1, "0", "1", ""),
            owned("row_added", "", 2, "", "", ""),
            owned("fact", "description", 2, "", "Post", ""),
            owned("fact", "quantity", 2, "", "4", ""),
            owned("fact", "unit_price", 2, "", "6", ""),
        ]
    );
    assert!(spielorders_plan_operation_count(&plan) > 0);
    assert_eq!(
        string(spielorders_constant_name(spielorders_plan_operation_kind(
            &plan, 0
        ))),
        "replace_order_record"
    );

    let report = commit(&plan);
    assert_eq!(
        spielorders_report_kind(&report),
        SPIELORDERS_REPORT_COMMITTED
    );
    let saved = spielorders_report_order(&report).unwrap();
    assert_eq!(
        string(spielorders_order_field(
            &saved,
            SPIELORDERS_ORDER_FIELD_TOTAL
        )),
        "99"
    );
    assert_eq!(
        string(spielorders_order_item_field(
            &saved,
            0,
            SPIELORDERS_ITEM_FIELD_DESCRIPTION
        )),
        "Iris"
    );

    let mut outcome = None;
    let mut again = None;
    assert_eq!(
        spielorders_plan_commit(&plan, &mut again, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(
        codes(&outcome),
        ["preconditions_changed"],
        "a plan commits once"
    );
}

#[test]
fn a_refused_save_locates_every_error_and_a_stale_save_writes_nothing() {
    let root = initialized("refusals");
    let directory = created_order(&root.0, "Laser components", &[("Lens", "2", "10")]);
    let package = open(&root.0);
    let order = read_order(&package, &directory);

    let mut draft = order_draft(&order);
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, " ");
    set(&mut draft, SPIELORDERS_ORDER_FIELD_CURRENCY, "dollars");
    set_row(&mut draft, 0, SPIELORDERS_ITEM_FIELD_QUANTITY, "many");
    let mut outcome = None;
    assert_eq!(
        spielorders_draft_check(&draft, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(
        located(&outcome),
        [
            ("invalid_assignments".to_owned(), "title".to_owned(), -1),
            ("invalid_fact_value".to_owned(), "currency".to_owned(), -1),
            ("invalid_assignments".to_owned(), "quantity".to_owned(), 0),
        ]
    );
    assert_eq!(
        spielorders_entries_reason(outcome.as_deref(), 2),
        SPIELORDERS_REASON_NOT_A_FINITE_NUMBER
    );
    let refused = plan_draft(&draft).map(|_| ()).unwrap_err();
    assert_eq!(
        refused,
        located(&outcome),
        "planning refuses as checking does"
    );

    let mut stale = order_draft(&order);
    set(&mut stale, SPIELORDERS_ORDER_FIELD_TITLE, "Stale title");
    let stale_plan = plan_draft(&stale).unwrap();
    let mut fresh = order_draft(&read_order(&package, &directory));
    set(&mut fresh, SPIELORDERS_ORDER_FIELD_TITLE, "Fresh title");
    commit(&plan_draft(&fresh).unwrap());
    let before = fs::read(directory.join(".spielorders/order.json")).unwrap();
    let mut report = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_plan_commit(&stale_plan, &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(report.is_none());
    assert_eq!(codes(&outcome), ["preconditions_changed"]);
    assert_eq!(
        fs::read(directory.join(".spielorders/order.json")).unwrap(),
        before
    );
}

#[test]
fn a_cancelled_call_returns_cancelled_and_a_released_token_cancels_nothing() {
    let root = initialized("cancel");
    let directory = created_order(&root.0, "Laser components", &[]);
    let package = open(&root.0);
    let directory = c_path(&directory);

    let token = spielorders_cancel_new();
    assert_ne!(token, 0);
    spielorders_cancel_request(token);
    let mut order = None;
    assert_eq!(
        spielorders_package_read_order(
            &package,
            PathBytes::new(&directory),
            token,
            &mut order,
            None
        ),
        SPIELORDERS_RESULT_CANCELLED
    );
    assert!(order.is_none());
    let root_text = c_path(&root.0);
    let mut opened = None;
    assert_eq!(
        spielorders_package_open(PathBytes::new(&root_text), token, &mut opened, None),
        SPIELORDERS_RESULT_CANCELLED
    );
    spielorders_cancel_release(token);

    let released = spielorders_cancel_new();
    spielorders_cancel_release(released);
    spielorders_cancel_request(released);
    assert_eq!(
        spielorders_package_read_order(
            &package,
            PathBytes::new(&directory),
            released,
            &mut order,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    assert_ne!(
        spielorders_cancel_new(),
        released,
        "a token is never reused"
    );
}

/// A new draft from `package`, Order or Vendor.
fn new_draft(
    package: &SpielordersPackage,
    start: extern "C" fn(
        &SpielordersPackage,
        &mut Option<Box<SpielordersDraft>>,
        Option<&mut Option<Box<SpielordersEntries>>>,
    ) -> SpielordersResult,
) -> Box<SpielordersDraft> {
    let mut draft = None;
    assert_eq!(start(package, &mut draft, None), SPIELORDERS_RESULT_OK);
    draft.unwrap()
}

#[test]
fn new_drafts_start_with_a_name_that_saves_as_it_is() {
    let root = initialized("default-names");
    let package = open(&root.0);
    let order = new_draft(&package, spielorders_package_new_order_draft);
    assert_eq!(
        string(spielorders_draft_field(
            &order,
            SPIELORDERS_ORDER_FIELD_TITLE
        )),
        "New Order"
    );
    commit(&plan_draft(&order).unwrap());

    let vendor = new_draft(&package, spielorders_package_new_vendor_draft);
    assert_eq!(
        string(spielorders_draft_field(
            &vendor,
            SPIELORDERS_VENDOR_FIELD_NAME
        )),
        "New Vendor"
    );
    let mut lower = vendor;
    set(&mut lower, SPIELORDERS_VENDOR_FIELD_NAME, "new vendor");
    commit(&plan_draft(&lower).unwrap());

    let package = open(&root.0);
    let next = new_draft(&package, spielorders_package_new_vendor_draft);
    assert_eq!(
        string(spielorders_draft_field(
            &next,
            SPIELORDERS_VENDOR_FIELD_NAME
        )),
        "New Vendor 2",
        "a Name already used in any case is passed over"
    );
    commit(&plan_draft(&next).unwrap());
}

#[test]
fn vendors_are_created_edited_with_contacts_and_hidden_through_drafts() {
    let root = initialized("vendors");
    let package = open(&root.0);
    let mut draft = None;
    assert_eq!(
        spielorders_package_new_vendor_draft(&package, &mut draft, None),
        SPIELORDERS_RESULT_OK
    );
    let mut draft = draft.unwrap();
    set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, "Tarnwick");
    let report = commit(&plan_draft(&draft).unwrap());
    let vendor = spielorders_report_vendor(&report).unwrap();
    let id = string(spielorders_vendor_field(
        &vendor,
        SPIELORDERS_VENDOR_FIELD_VENDOR_ID,
    ));

    let mut draft = None;
    assert_eq!(
        spielorders_vendor_draft(&vendor, &mut draft),
        SPIELORDERS_RESULT_OK
    );
    let mut draft = draft.unwrap();
    assert_eq!(
        spielorders_draft_row_insert(&mut draft, 0, None),
        SPIELORDERS_RESULT_OK
    );
    set_row(
        &mut draft,
        0,
        SPIELORDERS_CONTACT_FIELD_NAME,
        "Grace Hopper",
    );
    set(
        &mut draft,
        SPIELORDERS_VENDOR_FIELD_PHONE,
        "+1 555 0123",
    );
    commit(&plan_draft(&draft).unwrap());

    let package = open(&root.0);
    let id_text = c(&id);
    let mut vendor = None;
    assert_eq!(
        spielorders_package_read_vendor(
            &package,
            SPIELORDERS_VENDOR_SELECTOR_ID,
            Text::new(&id_text),
            0,
            &mut vendor,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    let vendor = vendor.unwrap();
    assert_eq!(spielorders_vendor_contact_count(&vendor), 1);
    assert_eq!(
        string(spielorders_vendor_contact_field(
            &vendor,
            0,
            SPIELORDERS_CONTACT_FIELD_NAME
        )),
        "Grace Hopper"
    );
    assert_eq!(
        string(spielorders_vendor_field(
            &vendor,
            SPIELORDERS_VENDOR_FIELD_PHONE
        )),
        "+1 555 0123"
    );

    let mut plan = None;
    assert_eq!(
        spielorders_vendor_plan_hidden(&vendor, 1, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    commit(&plan.unwrap());
    let package = open(&root.0);
    let mut offered = None;
    spielorders_package_vendors(
        &package,
        OptionalText::null(),
        0,
        SpielordersConstant(-1),
        0,
        0,
        &mut offered,
        None,
    );
    assert_eq!(spielorders_list_count(&offered.unwrap()), 0);
    let mut all = None;
    spielorders_package_vendors(
        &package,
        OptionalText::null(),
        SPIELORDERS_LIST_INCLUDE_HIDDEN,
        SpielordersConstant(-1),
        0,
        0,
        &mut all,
        None,
    );
    let all = all.unwrap();
    assert_eq!(
        string(spielorders_list_field(
            &all,
            0,
            SPIELORDERS_VENDOR_FIELD_HIDDEN
        )),
        "true"
    );
}

#[test]
fn a_readme_is_written_against_the_text_it_was_read_with() {
    let root = initialized("readme");
    let package = open(&root.0);
    let mut readme = None;
    assert_eq!(
        spielorders_package_read_readme(&package, 0, &mut readme, None),
        SPIELORDERS_RESULT_OK
    );
    let readme = readme.unwrap();
    let text = c("# Lab orders\n");
    let mut plan = None;
    assert_eq!(
        spielorders_readme_plan_write(&readme, Text::new(&text), 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    let plan = plan.unwrap();
    fs::write(root.0.join("README.md"), "edited elsewhere\n").unwrap();
    let mut report = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_plan_commit(&plan, &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(
        fs::read_to_string(root.0.join("README.md")).unwrap(),
        "edited elsewhere\n"
    );

    let mut fresh = None;
    spielorders_package_read_readme(&package, 0, &mut fresh, None);
    let fresh = fresh.unwrap();
    assert_eq!(
        string(spielorders_readme_text(&fresh)),
        "edited elsewhere\n"
    );
    let mut plan = None;
    spielorders_readme_plan_write(&fresh, Text::new(&text), 0, &mut plan, None);
    commit(&plan.unwrap());
    assert_eq!(
        fs::read_to_string(root.0.join("README.md")).unwrap(),
        "# Lab orders\n"
    );
}

#[test]
fn a_damaged_order_is_diagnosed_and_repaired_by_replacing_its_record() {
    let root = initialized("repair");
    let directory = created_order(&root.0, "Laser components", &[]);
    let record = directory.join(".spielorders/order.json");
    let good = fs::read_to_string(&record).unwrap();
    fs::write(&record, good.replacen('{', "{\"colour\": \"red\",", 1)).unwrap();
    let package = open(&root.0);
    let order_directory = c_path(&directory);

    let mut diagnosis = None;
    assert_eq!(
        spielorders_package_diagnose_order(
            &package,
            PathBytes::new(&order_directory),
            0,
            &mut diagnosis,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    let diagnosis = diagnosis.unwrap();
    assert_eq!(spielorders_diagnosis_case_count(&diagnosis), 1);
    assert_eq!(
        spielorders_diagnosis_case_code(&diagnosis, 0),
        SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SHAPE
    );
    let action = (0..spielorders_diagnosis_action_count(&diagnosis, 0))
        .find(|a| {
            spielorders_diagnosis_action(&diagnosis, 0, *a)
                == SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD
        })
        .expect("a whole-record replacement is offered");
    assert_eq!(
        spielorders_diagnosis_action_field(&diagnosis, 0, action, 0),
        SPIELORDERS_TEXT_FIELD_CONTENT
    );

    let mut draft = None;
    assert_eq!(
        spielorders_diagnosis_draft(&diagnosis, 0, action, &mut draft, None),
        SPIELORDERS_RESULT_OK
    );
    let mut draft = draft.unwrap();
    assert_eq!(spielorders_draft_kind(&draft), SPIELORDERS_DRAFT_KIND_TEXT);
    set(&mut draft, SPIELORDERS_TEXT_FIELD_CONTENT, &good);
    let mut plan = None;
    let mut outcome = None;
    let result = spielorders_diagnosis_plan_repair(
        &diagnosis,
        0,
        action,
        Some(&draft),
        0,
        &mut plan,
        Some(&mut outcome),
    );
    assert_eq!(result, SPIELORDERS_RESULT_OK, "{:?}", located(&outcome));
    commit(&plan.unwrap());

    let mut validated = None;
    spielorders_package_validate_order(
        &package,
        PathBytes::new(&order_directory),
        0,
        &mut validated,
        None,
    );
    assert_eq!(spielorders_diagnosis_is_valid(&validated.unwrap()), 1);
}

#[test]
fn an_order_is_aligned_and_removed_to_the_removed_orders_directory() {
    let root = initialized("remove");
    let directory = created_order(&root.0, "Laser components", &[]);
    let package = open(&root.0);
    let renamed = root.0.join("Orders/Old name");
    fs::rename(&directory, &renamed).unwrap();
    let order = read_order(&package, &renamed);
    let mut plan = None;
    assert_eq!(
        spielorders_order_plan_align(&order, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    let report = commit(&plan.unwrap());
    assert_eq!(path(spielorders_report_final_path(&report)), directory);

    let order = read_order(&package, &directory);
    let mut plan = None;
    assert_eq!(
        spielorders_order_plan_remove(&order, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    let report = commit(&plan.unwrap());
    assert!(path(spielorders_report_final_path(&report)).starts_with(root.0.join("Removed")));
    assert!(spielorders_report_order(&report).is_none());
    assert!(!directory.exists());
}

/// Supplies `hits` (path, kind, modified, rank) from a provider in `state`, and answers
/// the search or the entries it was refused with.
fn searched(
    package: &SpielordersPackage,
    hits: &[(&Path, &str, &str, &str)],
    said: &[&str],
    state: SpielordersConstant,
) -> Result<Box<SpielordersSearch>, Vec<(String, String)>> {
    let mut supplied = None;
    assert_eq!(
        spielorders_hits_new(Text::new(c"spotlight"), &mut supplied),
        SPIELORDERS_RESULT_OK
    );
    let mut supplied = supplied.unwrap();
    for (hit, kind, modified, rank) in hits {
        let (hit, kind, modified, rank) = (c_path(hit), c(kind), c(modified), c(rank));
        assert_eq!(
            spielorders_hits_add(
                &mut supplied,
                PathBytes::new(&hit),
                Text::new(&kind),
                Text::new(&modified),
                Text::new(&rank),
            ),
            SPIELORDERS_RESULT_OK
        );
    }
    for words in said {
        let words = c(words);
        assert_eq!(
            spielorders_hits_diagnostic(&mut supplied, Text::new(&words)),
            SPIELORDERS_RESULT_OK
        );
    }
    let mut search = None;
    let mut outcome = None;
    let result = spielorders_package_search_material(
        package,
        Some(&supplied),
        state,
        0,
        &mut search,
        Some(&mut outcome),
    );
    spielorders_hits_free(Some(supplied));
    if result == SPIELORDERS_RESULT_OK {
        return Ok(search.unwrap());
    }
    assert_eq!(result, SPIELORDERS_RESULT_REFUSED);
    let entries = outcome.as_deref();
    Err((0..spielorders_entries_count(entries))
        .map(|i| {
            (
                string(spielorders_constant_name(spielorders_entries_code(
                    entries, i,
                ))),
                string(spielorders_entries_message(entries, i)),
            )
        })
        .collect())
}

#[test]
fn supplied_hits_come_back_eligible_grouped_and_ordered() {
    let root = initialized("material");
    created_order(&root.0, "Laser components", &[]);
    created_order(&root.0, "Mirror mounts", &[]);
    let damaged = created_order(&root.0, "Breadboard", &[]);
    fs::write(damaged.join(".spielorders/order.json"), "{").unwrap();
    let package = open(&root.0);
    assert_eq!(
        path(spielorders_package_material_directory(&package)),
        root.0.join("Orders"),
        "one query over the Orders Directory"
    );
    let all = orders(&package, None, 0, SpielordersConstant(-1), 0);
    let first = path(spielorders_list_path(&all, 0));
    let second = path(spielorders_list_path(&all, 1));

    let quote = first.join("quote.pdf");
    let hits = [
        (second.join("notes.txt"), "txt", "1784937600", ""),
        (first.join("README.md"), "md", "", ""),
        (first.join(".spielorders/order.json"), "json", "", ""),
        (damaged.join("drawing.pdf"), "pdf", "", ""),
        (root.0.join("Orders/loose.txt"), "txt", "", ""),
        (root.0.join("elsewhere.txt"), "txt", "", ""),
        (first.join("b-unranked.txt"), "", "", ""),
        (first.join("a-unranked.txt"), "", "", ""),
        (first.join("vendor-pack/README.md"), "md", "", "0.25"),
        (quote.clone(), "pdf", "-86400", "0.75"),
        (quote.clone(), "pdf", "-86400", "0.75"),
    ];
    let hits: Vec<_> = hits
        .iter()
        .map(|(hit, kind, modified, rank)| (hit.as_path(), *kind, *modified, *rank))
        .collect();
    let search = searched(&package, &hits, &[], SPIELORDERS_PROVIDER_STATE_COMPLETE).unwrap();

    assert_eq!(string(spielorders_search_provider(&search)), "spotlight");
    assert_eq!(
        spielorders_search_state(&search),
        SPIELORDERS_PROVIDER_STATE_COMPLETE
    );
    let found: Vec<(PathBuf, PathBuf, String, String, String, String, String)> = (0
        ..spielorders_search_count(&search))
        .map(|i| {
            (
                path(spielorders_search_order_directory(&search, i)),
                path(spielorders_search_path(&search, i)),
                string(spielorders_search_relative_path(&search, i)),
                string(spielorders_search_filename(&search, i)),
                string(spielorders_search_kind(&search, i)),
                string(spielorders_search_modified(&search, i)),
                string(spielorders_search_rank(&search, i)),
            )
        })
        .collect();
    let row = |order: &Path, relative: &str, kind: &str, modified: &str, rank: &str| {
        let file = order.join(relative);
        let filename = file.file_name().unwrap().to_str().unwrap().to_owned();
        (
            order.to_path_buf(),
            file,
            relative.to_owned(),
            filename,
            kind.to_owned(),
            modified.to_owned(),
            rank.to_owned(),
        )
    };
    assert_eq!(
        found,
        [
            row(&first, "quote.pdf", "pdf", "-86400", "0.75"),
            row(&first, "vendor-pack/README.md", "md", "", "0.25"),
            row(&first, "a-unranked.txt", "", "", ""),
            row(&first, "b-unranked.txt", "", "", ""),
            row(&second, "notes.txt", "txt", "1784937600", ""),
        ],
        "each Order's files once, by rank then filename, under the Package's Order order; \
         no managed document, Damaged Order, or path outside every Order"
    );
    assert_eq!(
        path(spielorders_search_path(&search, 5)),
        PathBuf::new(),
        "out of range"
    );
}

#[test]
fn a_provider_that_could_not_look_refuses_in_its_own_words() {
    let root = initialized("material-refused");
    created_order(&root.0, "Laser components", &[]);
    let package = open(&root.0);

    assert_eq!(
        searched(
            &package,
            &[],
            &["Indexing is off for this volume"],
            SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED,
        )
        .err()
        .expect("a provider that could not look refuses"),
        [
            (
                "indexing_disabled".to_owned(),
                "Indexing disabled".to_owned()
            ),
            (
                "indexing_disabled".to_owned(),
                "Indexing disabled: Indexing is off for this volume".to_owned()
            ),
        ]
    );
    let timed_out = searched(
        &package,
        &[],
        &["The index stopped early"],
        SPIELORDERS_PROVIDER_STATE_TIMED_OUT,
    )
    .unwrap();
    assert_eq!(
        spielorders_search_state(&timed_out),
        SPIELORDERS_PROVIDER_STATE_TIMED_OUT
    );
    assert_eq!(spielorders_search_count(&timed_out), 0);
    assert_eq!(spielorders_search_diagnostic_count(&timed_out), 1);
    assert_eq!(
        string(spielorders_search_diagnostic(&timed_out, 0)),
        "The index stopped early",
        "an answer carries what the provider said beside it"
    );

    let mut search = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_package_search_material(
            &package,
            None,
            SPIELORDERS_PROVIDER_STATE_COMPLETE,
            0,
            &mut search,
            Some(&mut outcome)
        ),
        SPIELORDERS_RESULT_REFUSED,
        "a front end with no provider for its platform"
    );
    assert_eq!(codes(&outcome), ["unsupported"]);

    let mut supplied = None;
    spielorders_hits_new(Text::new(c"spotlight"), &mut supplied);
    let mut supplied = supplied.unwrap();
    let hit = c_path(&root.0.join("Orders/x.txt"));
    assert_eq!(
        spielorders_hits_add(
            &mut supplied,
            PathBytes::new(&hit),
            Text::new(c""),
            Text::new(c"yesterday"),
            Text::new(c""),
        ),
        SPIELORDERS_RESULT_BAD_ARGUMENT,
        "a modification time is decimal Unix seconds"
    );
    assert_eq!(
        spielorders_package_search_material(
            &package,
            Some(&supplied),
            SPIELORDERS_ORDER_STATUS_ORDERED,
            0,
            &mut search,
            None
        ),
        SPIELORDERS_RESULT_BAD_ARGUMENT,
        "a state of another family"
    );
}

/// A hit is judged by the file its path names, however the provider spelled that path.
///
/// Spotlight reports paths in decomposed Unicode, while the Order Directory this library
/// created for `Café` is stored composed. Where the volume makes the two spellings one
/// entry, as APFS does, the decomposed hit is that Order's file: it is matched, the Order's
/// managed documents stay excluded in that spelling, and the file reported in both
/// spellings is one result, answered in the Package's own spelling. Where the volume keeps
/// the spellings apart, the decomposed path names nothing, and nothing is matched to it.
#[test]
fn a_hit_is_judged_by_the_file_its_path_names_however_it_is_spelled() {
    let root = initialized("material-spelling");
    let order = created_order(&root.0, "Caf\u{e9}", &[]);
    let quote = order.join("quote.pdf");
    fs::write(&quote, "").unwrap();
    let package = open(&root.0);
    let decomposed =
        |path: &Path| PathBuf::from(path.to_str().unwrap().replace('\u{e9}', "e\u{301}"));
    let respelled = decomposed(&quote);
    assert_ne!(respelled, quote, "the Order Directory's name is composed");
    let one_entry = fs::symlink_metadata(&respelled).is_ok();
    let matched = |hits: &[&Path]| {
        let hits: Vec<_> = hits.iter().map(|hit| (*hit, "pdf", "", "")).collect();
        let search = searched(&package, &hits, &[], SPIELORDERS_PROVIDER_STATE_COMPLETE).unwrap();
        (0..spielorders_search_count(&search))
            .map(|i| {
                (
                    path(spielorders_search_order_directory(&search, i)),
                    path(spielorders_search_path(&search, i)),
                    string(spielorders_search_relative_path(&search, i)),
                )
            })
            .collect::<Vec<_>>()
    };
    let the_quote = vec![(order.clone(), quote.clone(), "quote.pdf".to_owned())];

    assert_eq!(
        matched(&[&respelled]),
        if one_entry {
            the_quote.clone()
        } else {
            Vec::new()
        },
        "the decomposed hit names the composed Order's file exactly when the volume says so"
    );
    assert_eq!(
        matched(&[
            &respelled,
            &decomposed(&order.join("README.md")),
            &decomposed(&order.join(".spielorders/order.json")),
            &quote,
        ]),
        the_quote,
        "managed documents stay excluded in either spelling, and one file is one result"
    );
}

#[test]
fn a_package_with_no_healthy_order_has_no_directory_to_search() {
    let root = initialized("material-none");
    let package = open(&root.0);
    assert_eq!(
        path(spielorders_package_material_directory(&package)),
        PathBuf::new()
    );
}

#[test]
fn reusable_values_and_entries_read_through_their_accessors() {
    let root = initialized("values");
    let directory = created_order(&root.0, "Laser components", &[]);
    let package = open(&root.0);
    let mut draft = order_draft(&read_order(&package, &directory));
    set(&mut draft, SPIELORDERS_ORDER_FIELD_ORDERER, "Ada Lovelace");
    let refused = plan_draft(&draft).map(|_| ()).unwrap_err();
    assert_eq!(
        refused,
        [(
            "unconfirmed_reusable_value".to_owned(),
            "orderer".to_owned(),
            -1
        )]
    );
    spielorders_draft_confirm(&mut draft, SPIELORDERS_ORDER_FIELD_ORDERER, -1, None);
    commit(&plan_draft(&draft).unwrap());

    let package = open(&root.0);
    let mut list = None;
    assert_eq!(
        spielorders_package_values(&package, SPIELORDERS_ORDER_FIELD_ORDERER, &mut list, None),
        SPIELORDERS_RESULT_OK
    );
    let list = list.unwrap();
    assert_eq!(
        string(spielorders_list_field(
            &list,
            0,
            SPIELORDERS_VALUE_FIELD_VALUE
        )),
        "Ada Lovelace"
    );
    assert_eq!(
        string(spielorders_list_field(
            &list,
            0,
            SPIELORDERS_VALUE_FIELD_USAGE_COUNT
        )),
        "1"
    );

    assert_eq!(spielorders_entries_count(None), 0);
    assert_eq!(spielorders_entries_code(None, 0), SpielordersConstant(-1));
    assert_eq!(string(spielorders_entries_message(None, 0)), "");
}

#[test]
fn every_named_value_reads_back_in_exactly_one_family() {
    let families: Vec<SpielordersConstant> = (100..200)
        .map(SpielordersConstant)
        .filter(|family| !string(spielorders_constant_name(*family)).is_empty())
        .collect();
    let mut named = 0;
    for value in (0..40_000).map(SpielordersConstant) {
        let name = spielorders_constant_name(value);
        if string(name).is_empty() || families.contains(&value) {
            continue;
        }
        named += 1;
        let homes = families
            .iter()
            .filter(|family| spielorders_constant_from_name(**family, name) == value)
            .count();
        assert_eq!(homes, 1, "{}", string(name));
    }
    assert!(named > 200);
}

#[test]
fn a_new_order_saves_its_item_rows_and_refuses_with_every_fault_located() {
    let root = initialized("new-order-rows");
    let package = open(&root.0);
    let mut draft = None;
    spielorders_package_new_order_draft(&package, &mut draft, None);
    let mut draft = draft.unwrap();
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "");
    for row in 0..2 {
        assert_eq!(
            spielorders_draft_row_insert(&mut draft, row, None),
            SPIELORDERS_RESULT_OK
        );
    }
    set_row(&mut draft, 0, SPIELORDERS_ITEM_FIELD_DESCRIPTION, "Lens");
    set_row(&mut draft, 0, SPIELORDERS_ITEM_FIELD_QUANTITY, "2");
    set_row(&mut draft, 0, SPIELORDERS_ITEM_FIELD_UNIT_PRICE, "10");
    set_row(&mut draft, 1, SPIELORDERS_ITEM_FIELD_DESCRIPTION, "Iris");
    set_row(&mut draft, 1, SPIELORDERS_ITEM_FIELD_QUANTITY, "one");
    set_row(&mut draft, 1, SPIELORDERS_ITEM_FIELD_UNIT_PRICE, "55");
    assert_eq!(
        plan_draft(&draft).map(|_| ()).unwrap_err(),
        [
            ("invalid_assignments".to_owned(), "title".to_owned(), -1),
            ("invalid_assignments".to_owned(), "quantity".to_owned(), 1),
        ]
    );

    set(
        &mut draft,
        SPIELORDERS_ORDER_FIELD_TITLE,
        "Laser components",
    );
    set_row(&mut draft, 1, SPIELORDERS_ITEM_FIELD_QUANTITY, "1");
    let report = commit(&plan_draft(&draft).unwrap());
    let order = spielorders_report_order(&report).unwrap();
    assert_eq!(spielorders_order_item_count(&order), 2);
    assert_eq!(
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_TOTAL
        )),
        "75"
    );
}

#[test]
fn a_new_vendor_saves_its_contact_rows_in_one_commit_and_refuses_with_every_fault_located() {
    let root = initialized("new-vendor-rows");
    let package = open(&root.0);
    let mut draft = None;
    spielorders_package_new_vendor_draft(&package, &mut draft, None);
    let mut draft = draft.unwrap();
    set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, "");
    for row in 0..2 {
        assert_eq!(
            spielorders_draft_row_insert(&mut draft, row, None),
            SPIELORDERS_RESULT_OK
        );
    }
    set_row(
        &mut draft,
        0,
        SPIELORDERS_CONTACT_FIELD_NAME,
        "Grace Hopper",
    );
    set_row(&mut draft, 0, SPIELORDERS_CONTACT_FIELD_ROLE, "Engineering");
    set_row(&mut draft, 1, SPIELORDERS_CONTACT_FIELD_ROLE, "Accounts");
    let refused = plan_draft(&draft).map(|_| ()).unwrap_err();
    assert!(
        refused
            .iter()
            .any(|(_, field, row)| field == "name" && *row == -1)
            && refused
                .iter()
                .any(|(_, field, row)| field == "name" && *row == 1),
        "the Vendor's own Name and the second Contact's are both owed: {refused:?}"
    );
    assert!(
        !std::fs::read_to_string(root.0.join(".spielorders/vendors.json"))
            .unwrap()
            .contains("Grace Hopper"),
        "a refused save writes nothing"
    );

    set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, "Tarnwick");
    set_row(
        &mut draft,
        1,
        SPIELORDERS_CONTACT_FIELD_NAME,
        "Katherine Johnson",
    );
    let plan = plan_draft(&draft).unwrap();
    assert_eq!(
        (0..spielorders_plan_operation_count(&plan))
            .filter(|index| {
                spielorders_plan_operation_kind(&plan, *index)
                    == SPIELORDERS_OPERATION_REPLACE_VENDOR_RECORD
            })
            .count(),
        1,
        "the Vendor and its Contacts are one replacement of the Vendor record"
    );
    let report = commit(&plan);
    let vendor = spielorders_report_vendor(&report).unwrap();
    assert_eq!(spielorders_vendor_contact_count(&vendor), 2);
    assert_eq!(
        string(spielorders_vendor_contact_field(
            &vendor,
            1,
            SPIELORDERS_CONTACT_FIELD_NAME
        )),
        "Katherine Johnson"
    );
}
