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
    initialized_in(label, OptionalPathBytes::null())
}

fn initialized_in(label: &str, orders: OptionalPathBytes<'_>) -> Directory {
    let directory = Directory::new(label);
    let mut plan = None;
    let root = c_path(&directory.0);
    assert_eq!(
        spielorders_plan_initialize(
            PathBytes::new(&root),
            orders,
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

/// A planning call's result, its plan, and its entries.
type Planned = (
    SpielordersResult,
    Option<Box<SpielordersPlan>>,
    Option<Box<SpielordersEntries>>,
);

/// Plans agent provisioning into `directory` with `token`.
fn provision(directory: &Path, token: u64) -> Planned {
    let directory = c_path(directory);
    let mut plan = None;
    let mut outcome = None;
    let result = spielorders_provision_agent(
        PathBytes::new(&directory),
        token,
        &mut plan,
        Some(&mut outcome),
    );
    (result, plan, outcome)
}

/// Every path under `root` with its file bytes, or `None` for a directory or link.
fn tree(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut entries = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            let kind = fs::symlink_metadata(&path).unwrap().file_type();
            if kind.is_dir() {
                pending.push(path.clone());
            }
            let bytes = kind.is_file().then(|| fs::read(&path).unwrap());
            entries.push((path.strip_prefix(root).unwrap().to_path_buf(), bytes));
        }
    }
    entries.sort();
    entries
}

fn bundled(relative: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("agent-guidance")
            .join(relative),
    )
    .unwrap()
}

#[test]
fn provisioning_writes_the_current_guidance_and_leaves_every_other_file() {
    let directory = Directory::new("agent-ordinary");
    let root = &directory.0;
    let skill = root.join(".agents/skills/spielorders");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "custom skill\n").unwrap();
    fs::write(skill.join("local.md"), "added by the owner\n").unwrap();
    fs::write(root.join("AGENTS.md"), "owner instructions\n").unwrap();
    fs::write(root.join("notes.txt"), "owner notes\n").unwrap();
    let before = tree(root);

    let (result, plan, outcome) = provision(root, 0);
    assert_eq!(result, SPIELORDERS_RESULT_OK, "{:?}", located(&outcome));
    assert_eq!(tree(root), before, "planning writes nothing");
    assert_eq!(
        spielorders_report_kind(&commit(&plan.unwrap())),
        SPIELORDERS_REPORT_COMMITTED
    );
    assert_eq!(
        fs::read(skill.join("references/orders-and-items.md")).unwrap(),
        bundled("references/orders-and-items.md")
    );
    assert_eq!(
        fs::read(skill.join("SKILL.md")).unwrap(),
        bundled("SKILL.md"),
        "a guidance file this application ships is replaced with the current one"
    );
    assert_eq!(
        fs::read_to_string(skill.join("local.md")).unwrap(),
        "added by the owner\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "owner instructions\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("notes.txt")).unwrap(),
        "owner notes\n"
    );
    assert!(!root.join(".spielorders").exists(), "no Package is created");

    let provisioned = tree(root);
    let (result, plan, _) = provision(root, 0);
    assert_eq!(result, SPIELORDERS_RESULT_OK);
    assert_eq!(
        spielorders_report_kind(&commit(&plan.unwrap())),
        SPIELORDERS_REPORT_UNCHANGED
    );
    assert_eq!(tree(root), provisioned, "repeating changes nothing");
    assert!(
        string(spielorders_library_agent_setup_prompt())
            .contains(".agents/skills/spielorders/SKILL.md")
    );
}

#[test]
fn provisioning_neither_requires_nor_reads_package_metadata() {
    let directory = initialized("agent-damaged");
    let root = &directory.0;
    fs::write(root.join(".spielorders/config.toml"), "orders_path = [").unwrap();
    fs::write(root.join(".spielorders/version.json"), "not json").unwrap();
    let metadata = tree(&root.join(".spielorders"));

    let (result, plan, outcome) = provision(root, 0);
    assert_eq!(result, SPIELORDERS_RESULT_OK, "{:?}", located(&outcome));
    commit(&plan.unwrap());
    assert_eq!(
        fs::read(root.join(".agents/skills/spielorders/SKILL.md")).unwrap(),
        bundled("SKILL.md")
    );
    assert_eq!(
        tree(&root.join(".spielorders")),
        metadata,
        "metadata is untouched"
    );
}

#[test]
fn provisioning_refuses_unsuitable_destinations_without_writing() {
    use std::os::unix::fs::symlink;
    let outside = Directory::new("agent-outside");
    fs::write(outside.0.join("SKILL.md"), "private").unwrap();
    let directory = Directory::new("agent-refusals");
    let root = &directory.0;

    symlink(root, outside.0.join("linked")).unwrap();
    let (result, _, outcome) = provision(&outside.0.join("linked"), 0);
    assert_eq!(result, SPIELORDERS_RESULT_REFUSED);
    assert_eq!(codes(&outcome), ["root_is_a_link"]);
    for missing in [root.join("absent"), outside.0.join("SKILL.md")] {
        let (result, _, outcome) = provision(&missing, 0);
        assert_eq!(result, SPIELORDERS_RESULT_REFUSED);
        assert_eq!(codes(&outcome), ["root_not_ordinary_directory"]);
    }

    let token = spielorders_cancel_new();
    spielorders_cancel_request(token);
    assert_eq!(provision(root, token).0, SPIELORDERS_RESULT_CANCELLED);
    spielorders_cancel_release(token);

    let skill = root.join(".agents/skills/spielorders");
    for conflict in [
        "file in place of a directory",
        "linked directory",
        "linked guidance file",
        "leftover staging file",
    ] {
        let _ = fs::remove_file(root.join(".agents"));
        let _ = fs::remove_dir_all(root.join(".agents"));
        match conflict {
            "file in place of a directory" => fs::write(root.join(".agents"), "owner").unwrap(),
            "linked directory" => symlink(&outside.0, root.join(".agents")).unwrap(),
            "linked guidance file" => {
                fs::create_dir_all(&skill).unwrap();
                symlink(outside.0.join("SKILL.md"), skill.join("SKILL.md")).unwrap();
            }
            _ => {
                fs::create_dir_all(&skill).unwrap();
                fs::write(skill.join(".SKILL.md.spielorders-stage"), "partial").unwrap();
            }
        }
        let before = tree(root);
        let (result, _, outcome) = provision(root, 0);
        assert_eq!(result, SPIELORDERS_RESULT_REFUSED, "{conflict}");
        assert_eq!(codes(&outcome), ["destination_collision"], "{conflict}");
        assert_eq!(tree(root), before, "{conflict}");
    }
    assert_eq!(
        fs::read_to_string(outside.0.join("SKILL.md")).unwrap(),
        "private"
    );
    assert_eq!(
        fs::read_dir(&outside.0).unwrap().count(),
        2,
        "nothing is written outside"
    );
}

#[test]
fn commit_never_replaces_entries_created_after_planning() {
    let directory = Directory::new("agent-concurrent");
    let root = &directory.0;
    let (_, plan, _) = provision(root, 0);
    fs::create_dir(root.join(".agents")).unwrap();
    fs::write(root.join(".agents/owner.txt"), "concurrent work").unwrap();
    let mut report = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_plan_commit(&plan.unwrap(), &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(
        tree(root),
        [
            (PathBuf::from(".agents"), None),
            (
                PathBuf::from(".agents/owner.txt"),
                Some(b"concurrent work".to_vec())
            ),
        ]
    );

    let skill = root.join(".agents/skills/spielorders");
    let (_, plan, _) = provision(root, 0);
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "concurrent skill").unwrap();
    assert_eq!(
        spielorders_plan_commit(&plan.unwrap(), &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(
        fs::read_to_string(skill.join("SKILL.md")).unwrap(),
        "concurrent skill"
    );
    assert!(!skill.join("references").exists());
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

/// Every family and every constant the header publishes in one is named as the header
/// spells it: its name is the macro's own suffix and the name its documentation opens
/// with, and the name reads back as that constant in that family and in no other.
#[test]
fn every_published_constant_is_named_as_its_header_says() {
    let header = include_str!("../include/spielorders.h");
    let lines: Vec<&str> = header.lines().collect();
    let defines: Vec<(usize, &str, i32)> = lines
        .iter()
        .enumerate()
        .filter_map(|(at, line)| {
            let mut words = line.strip_prefix("#define ")?.split_whitespace();
            let macro_name = words.next()?.strip_prefix("SPIELORDERS_")?;
            Some((at, macro_name, words.next()?.parse().ok()?))
        })
        .collect();
    let families: Vec<(String, SpielordersConstant)> = defines
        .iter()
        .filter_map(|(_, macro_name, value)| {
            let family = macro_name.strip_prefix("FAMILY_")?;
            Some((format!("{family}_"), SpielordersConstant(*value)))
        })
        .collect();
    for (prefix, family) in &families {
        assert_eq!(
            string(spielorders_constant_name(*family)),
            prefix.trim_end_matches('_').to_ascii_lowercase(),
            "SPIELORDERS_FAMILY_{prefix}"
        );
    }
    let mut members = 0;
    for (at, macro_name, value) in &defines {
        let Some((prefix, family)) = families
            .iter()
            .filter(|(prefix, _)| macro_name.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
        else {
            continue;
        };
        members += 1;
        let constant = SpielordersConstant(*value);
        let name = macro_name[prefix.len()..].to_ascii_lowercase();
        assert_eq!(
            string(spielorders_constant_name(constant)),
            name,
            "SPIELORDERS_{macro_name}"
        );
        let documented = lines[..*at]
            .iter()
            .rev()
            .take_while(|line| line.starts_with("//"))
            .filter_map(|line| line.strip_prefix("// `")?.split_once("`."))
            .map(|(documented, _)| documented)
            .last();
        assert_eq!(
            documented,
            Some(name.as_str()),
            "SPIELORDERS_{macro_name} opens its documentation with its name"
        );
        let spelled = c(&name);
        assert_eq!(
            spielorders_constant_from_name(*family, Text::new(&spelled)),
            constant,
            "SPIELORDERS_{macro_name} reads back in its family"
        );
        assert_eq!(
            families
                .iter()
                .filter(|(_, other)| {
                    spielorders_constant_from_name(*other, Text::new(&spelled)) == constant
                })
                .count(),
            1,
            "SPIELORDERS_{macro_name} reads back in no other family"
        );
    }
    assert!(members > families.len(), "the header publishes constants");
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
    set(&mut draft, SPIELORDERS_VENDOR_FIELD_PHONE, "+1 555 0123");
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

#[test]
fn a_commit_reports_the_package_it_left() {
    let root = initialized("report-package");
    let lenses = created_order(&root.0, "Lenses", &[]);
    let mounts = created_order(&root.0, "Mounts", &[]);
    let listed = |package: &SpielordersPackage| {
        let mut listed = titles(&orders(package, None, 0, SpielordersConstant(-1), 0));
        listed.sort();
        listed
    };

    let mut draft = order_draft(&read_order(&open(&root.0), &lenses));
    set(
        &mut draft,
        SPIELORDERS_ORDER_FIELD_TITLE,
        "Lenses and filters",
    );
    let report = commit(&plan_draft(&draft).unwrap());
    let package = spielorders_report_package(&report).unwrap();
    assert_eq!(listed(&package), ["Lenses and filters", "Mounts"]);

    let mut plan = None;
    assert_eq!(
        spielorders_order_plan_remove(&read_order(&package, &mounts), 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    let report = commit(&plan.unwrap());
    let package = spielorders_report_package(&report).unwrap();
    assert_eq!(listed(&package), ["Lenses and filters"]);
    assert_eq!(listed(&package), listed(&open(&root.0)));
}

/// The Package facts a front end can present without doing another disk read.
fn package_view(package: &SpielordersPackage) -> Vec<String> {
    let mut view = vec![format!(
        "{:?} {:?} {:?}",
        path(spielorders_package_root(package)),
        spielorders_package_state(package),
        spielorders_package_index_state(package)
    )];
    for i in 0..spielorders_package_index_reason_count(package) {
        view.push(format!(
            "{:?}",
            spielorders_package_index_reason(package, i)
        ));
    }
    let findings = spielorders_package_findings(package);
    for i in 0..spielorders_entries_count(findings.as_deref()) {
        view.push(format!(
            "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
            spielorders_entries_code(findings.as_deref(), i),
            spielorders_entries_severity(findings.as_deref(), i),
            string(spielorders_entries_message(findings.as_deref(), i)),
            spielorders_entries_field(findings.as_deref(), i),
            spielorders_entries_row(findings.as_deref(), i),
            string(spielorders_entries_path(findings.as_deref(), i)),
            spielorders_entries_reason(findings.as_deref(), i),
            string(spielorders_entries_before(findings.as_deref(), i)),
            string(spielorders_entries_after(findings.as_deref(), i)),
        ));
    }
    let listed = orders(package, None, 0, SpielordersConstant(-1), 0);
    for i in 0..spielorders_list_count(&listed) {
        view.push(format!(
            "order {:?} {}",
            path(spielorders_list_path(&listed, i)),
            spielorders_list_damaged(&listed, i)
        ));
        for field in [
            SPIELORDERS_ORDER_FIELD_ORDER_ID,
            SPIELORDERS_ORDER_FIELD_TITLE,
            SPIELORDERS_ORDER_FIELD_DATE_ORDERED,
            SPIELORDERS_ORDER_FIELD_STATUS,
            SPIELORDERS_ORDER_FIELD_VENDOR,
            SPIELORDERS_ORDER_FIELD_VENDOR_ID,
            SPIELORDERS_ORDER_FIELD_ORDERER,
            SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
            SPIELORDERS_ORDER_FIELD_CURRENCY,
            SPIELORDERS_ORDER_FIELD_PROJECT,
            SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
            SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME,
            SPIELORDERS_ORDER_FIELD_TOTAL,
            SPIELORDERS_ORDER_FIELD_DAMAGE,
        ] {
            view.push(string(spielorders_list_field(&listed, i, field)));
        }
    }
    for flags in [0, SPIELORDERS_LIST_INCLUDE_HIDDEN] {
        let mut vendors = None;
        assert_eq!(
            spielorders_package_vendors(
                package,
                OptionalText::null(),
                flags,
                SpielordersConstant(-1),
                0,
                0,
                &mut vendors,
                None
            ),
            SPIELORDERS_RESULT_OK
        );
        let vendors = vendors.unwrap();
        view.push(format!("vendors {flags}"));
        for i in 0..spielorders_list_count(&vendors) {
            for field in [
                SPIELORDERS_VENDOR_FIELD_VENDOR_ID,
                SPIELORDERS_VENDOR_FIELD_NAME,
                SPIELORDERS_VENDOR_FIELD_ADDRESS,
                SPIELORDERS_VENDOR_FIELD_EMAIL,
                SPIELORDERS_VENDOR_FIELD_PHONE,
                SPIELORDERS_VENDOR_FIELD_WEBSITE,
                SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES,
                SPIELORDERS_VENDOR_FIELD_HIDDEN,
            ] {
                view.push(string(spielorders_list_field(&vendors, i, field)));
            }
        }
    }
    for field in [
        SPIELORDERS_ORDER_FIELD_ORDERER,
        SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
        SPIELORDERS_ORDER_FIELD_PROJECT,
        SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
    ] {
        let mut values = None;
        assert_eq!(
            spielorders_package_values(package, field, &mut values, None),
            SPIELORDERS_RESULT_OK
        );
        let values = values.unwrap();
        view.push(format!("values {field:?}"));
        for i in 0..spielorders_list_count(&values) {
            view.push(string(spielorders_list_field(
                &values,
                i,
                SPIELORDERS_VALUE_FIELD_VALUE,
            )));
            view.push(string(spielorders_list_field(
                &values,
                i,
                SPIELORDERS_VALUE_FIELD_USAGE_COUNT,
            )));
        }
    }
    view.push(format!("groups {:?}", project_groups(package, None)));
    view.push(format!(
        "groups {:?}",
        project_groups(package, Some("Optics"))
    ));
    view.push(format!(
        "groups {:?}",
        project_groups(package, Some("Controls"))
    ));
    view
}

/// Repeated access gives independent handles that outlive the report and describe the
/// same lists, values, health, index, and findings as an ordinary open after the write.
fn reported_package(root: &Path, report: Box<SpielordersReport>) -> Box<SpielordersPackage> {
    let first = spielorders_report_package(&report)
        .expect("a healthy current-index refresh supplies a Package");
    let second = spielorders_report_package(&report).unwrap();
    spielorders_report_free(Some(report));
    let expected = package_view(&open(root));
    assert_eq!(package_view(&first), expected);
    spielorders_package_free(Some(first));
    assert_eq!(package_view(&second), expected);
    second
}

#[test]
fn reported_packages_match_fresh_opens_after_each_kind_of_change() {
    let root = initialized("report-package-changes");
    let kept = classified_order(&root.0, "Lens", "Optics", "Preparation");
    let other = classified_order(&root.0, "Controller", "Controls", "Assembly");
    let mut package = open(&root.0);
    let mut vendor = new_draft(&package, spielorders_package_new_vendor_draft);
    set(&mut vendor, SPIELORDERS_VENDOR_FIELD_NAME, "North Supplies");
    package = reported_package(&root.0, commit(&plan_draft(&vendor).unwrap()));
    let vendor = vendor_named(&package, "North Supplies").unwrap();
    let mut plan = None;
    assert_eq!(
        spielorders_vendor_plan_hidden(&vendor, 1, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    package = reported_package(&root.0, commit(&plan.unwrap()));

    let mut draft = order_draft(&read_order(&package, &kept));
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Lens and filter");
    package = reported_package(&root.0, commit(&plan_draft(&draft).unwrap()));

    let mut plan = None;
    assert_eq!(
        spielorders_order_plan_remove(&read_order(&package, &other), 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    let report = commit(&plan.unwrap());
    let removed = path(spielorders_report_final_path(&report));
    reported_package(&root.0, report);
    package = reported_package(&root.0, commit(&plan_restore(&root.0, &removed).unwrap()));

    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_rename_value(
            &package,
            SPIELORDERS_ORDER_FIELD_PROJECT,
            OptionalText::null(),
            Text::new(c"Controls"),
            Text::new(c"Electronics"),
            0,
            &mut plan,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    package = reported_package(&root.0, commit(&plan.unwrap()));
    let names = [Text::new(c"Optics"), Text::new(c"Electronics")];
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_merge_values(
            &package,
            SPIELORDERS_ORDER_FIELD_PROJECT,
            OptionalText::null(),
            std::ptr::NonNull::new(names.as_ptr().cast_mut()),
            2,
            Text::new(c"Optics"),
            0,
            &mut plan,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    package = reported_package(&root.0, commit(&plan.unwrap()));
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_delete_value(
            &package,
            SPIELORDERS_ORDER_FIELD_PROJECT,
            OptionalText::null(),
            Text::new(c"Optics"),
            OptionalText::new(c""),
            0,
            &mut plan,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    package = reported_package(&root.0, commit(&plan.unwrap()));

    let other_path = c_path(&other);
    let merged = [PathBytes::new(&other_path)];
    let mut plan = None;
    assert_eq!(
        spielorders_order_plan_merge(
            &read_order(&package, &kept),
            std::ptr::NonNull::new(merged.as_ptr().cast_mut()),
            1,
            0,
            &mut plan,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    reported_package(&root.0, commit(&plan.unwrap()));

    // Structural warnings and Package README notices must survive the reusable snapshot.
    fs::write(root.0.join("Orders/Unfiled.txt"), "unfiled note").unwrap();
    fs::remove_file(root.0.join("README.md")).unwrap();
    fs::create_dir(root.0.join("README.md")).unwrap();
    let mut draft = order_draft(&read_order(&open(&root.0), &kept));
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Lens components");
    let package = reported_package(&root.0, commit(&plan_draft(&draft).unwrap()));
    let findings = spielorders_package_findings(&package);
    assert!(codes(&findings).contains(&"readme_unavailable".to_owned()));
    assert!(spielorders_entries_count(findings.as_deref()) >= 2);

    // A damaged Order cannot be represented in a current index, so the caller reopens.
    let damaged = created_order(&root.0, "Damaged", &[]);
    fs::write(damaged.join(".spielorders/order.json"), "{broken").unwrap();
    let mut draft = order_draft(&read_order(&open(&root.0), &kept));
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Lens assembly");
    let report = commit(&plan_draft(&draft).unwrap());
    assert!(
        spielorders_report_package(&report).is_none(),
        "a damaged Order prevents a fresh open from proving the index current"
    );
    let fallback = open(&root.0);
    assert_eq!(
        spielorders_package_index_state(&fallback),
        SPIELORDERS_INDEX_STATE_STALE
    );
    assert_eq!(
        spielorders_list_count(&orders(&fallback, None, 0, SpielordersConstant(-1), 0)),
        2
    );
    assert!(
        titles(&orders(&fallback, None, 0, SpielordersConstant(-1), 0))
            .contains(&"Lens assembly".to_owned())
    );
    let findings = spielorders_package_findings(&fallback);
    assert!(codes(&findings).contains(&"managed_artifact_invalid_syntax".to_owned()));
    assert!(codes(&findings).contains(&"readme_unavailable".to_owned()));
}

#[test]
fn reports_without_a_refreshed_package_leave_reopening_to_the_caller() {
    let root = initialized("report-no-refresh");
    let directory = created_order(&root.0, "Lens", &[]);
    let mut draft = order_draft(&read_order(&open(&root.0), &directory));
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Filter");
    let plan = plan_draft(&draft).unwrap();
    fs::create_dir(root.0.join(".spielorders/.index.json.spielorders-stage")).unwrap();
    let mut report = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_plan_commit(&plan, &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_OK
    );
    let report = report.unwrap();
    assert_eq!(
        spielorders_report_kind(&report),
        SPIELORDERS_REPORT_COMMITTED
    );
    assert!(
        !codes(&outcome).is_empty(),
        "a failed index refresh remains a committed warning"
    );
    assert!(spielorders_report_package(&report).is_none());
    assert_eq!(
        titles(&orders(&open(&root.0), None, 0, SpielordersConstant(-1), 0)),
        ["Filter"]
    );

    // A refused or failed commit gives no report to read at all.
    let mut refused = None;
    assert_eq!(
        spielorders_plan_commit(&plan, &mut refused, None),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(refused.is_none());

    let destination = Directory::new("report-guidance-only");
    let (result, plan, _) = provision(&destination.0, 0);
    assert_eq!(result, SPIELORDERS_RESULT_OK);
    assert!(spielorders_report_package(&commit(&plan.unwrap())).is_none());
}

#[test]
fn an_unchanged_report_keeps_its_package_after_later_commits() {
    let root = initialized("report-snapshot");
    let directory = created_order(&root.0, "Lens", &[]);
    let draft = order_draft(&read_order(&open(&root.0), &directory));
    let report = commit(&plan_draft(&draft).unwrap());
    assert_eq!(
        spielorders_report_kind(&report),
        SPIELORDERS_REPORT_UNCHANGED
    );
    let before = package_view(&open(&root.0));
    created_order(&root.0, "Filter", &[]);
    let snapshot = spielorders_report_package(&report).unwrap();
    spielorders_report_free(Some(report));
    assert_eq!(package_view(&snapshot), before);
    assert_ne!(package_view(&snapshot), package_view(&open(&root.0)));
}

#[test]
fn a_nested_packages_report_describes_its_own_root() {
    let outer = initialized("report-outer");
    let nested = outer.0.join("Workspace");
    fs::create_dir(&nested).unwrap();
    let root = c_path(&nested);
    let mut plan = None;
    assert_eq!(
        spielorders_plan_initialize(
            PathBytes::new(&root),
            OptionalPathBytes::null(),
            OptionalPathBytes::null(),
            0,
            &mut plan,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    let report = commit(&plan.unwrap());
    let initialized = spielorders_report_package(&report).unwrap_or_else(|| open(&nested));
    assert_eq!(path(spielorders_package_root(&initialized)), nested);
    let directory = created_order(&nested, "Lens", &[]);
    let mut draft = order_draft(&read_order(&open(&nested), &directory));
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Filter");
    let package = reported_package(&nested, commit(&plan_draft(&draft).unwrap()));
    assert_eq!(path(spielorders_package_root(&package)), nested);
    assert_eq!(
        titles(&orders(
            &open(&outer.0),
            None,
            0,
            SpielordersConstant(-1),
            0
        )),
        Vec::<String>::new()
    );
}

#[test]
fn rebuilding_a_copied_package_uses_its_records_not_the_copied_index() {
    let source = initialized("copy-source");
    let directory = created_order(&source.0, "Lens", &[]);
    let old_index = fs::read(source.0.join(".spielorders/index.json")).unwrap();
    let mut draft = order_draft(&read_order(&open(&source.0), &directory));
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Filter");
    commit(&plan_draft(&draft).unwrap());

    let copied = Directory::new("copied-package");
    let index = Path::new(".spielorders/index.json");
    for (relative, bytes) in tree(&source.0) {
        if relative == index {
            continue;
        }
        let destination = copied.0.join(relative);
        if let Some(bytes) = bytes {
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(destination, bytes).unwrap();
        } else {
            fs::create_dir_all(destination).unwrap();
        }
    }
    // A restore can copy its old index last, giving it newer change evidence even though
    // the canonical records hold different facts. The documented rebuild reads them all.
    fs::write(copied.0.join(index), old_index).unwrap();
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_rebuild_index(&open(&copied.0), 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    let report = commit(&plan.unwrap());
    let package = spielorders_report_package(&report).unwrap_or_else(|| open(&copied.0));
    assert_eq!(package_view(&package), package_view(&open(&copied.0)));
    assert_eq!(
        titles(&orders(&package, None, 0, SpielordersConstant(-1), 0)),
        ["Filter"]
    );
    assert_eq!(
        titles(&orders(
            &open(&source.0),
            None,
            0,
            SpielordersConstant(-1),
            0
        )),
        ["Filter"]
    );
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
    let report = commit(&plan_draft(&draft).unwrap());
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

fn search_orders(
    package: &SpielordersPackage,
    query: &str,
    field: SpielordersConstant,
    flags: u32,
) -> Result<Vec<String>, SpielordersResult> {
    let query = c(query);
    let mut list = None;
    let result = spielorders_package_search_orders(
        package,
        OptionalText::new(&query),
        field,
        flags,
        SpielordersConstant(-1),
        0,
        0,
        &mut list,
        None,
    );
    if result != SPIELORDERS_RESULT_OK {
        return Err(result);
    }
    Ok(titles(&list.unwrap()))
}

fn search_vendors(
    package: &SpielordersPackage,
    query: &str,
    field: SpielordersConstant,
) -> Result<Vec<String>, SpielordersResult> {
    let query = c(query);
    let mut list = None;
    let result = spielorders_package_search_vendors(
        package,
        OptionalText::new(&query),
        field,
        0,
        SpielordersConstant(-1),
        0,
        0,
        &mut list,
        None,
    );
    if result != SPIELORDERS_RESULT_OK {
        return Err(result);
    }
    let list = list.unwrap();
    Ok((0..spielorders_list_count(&list))
        .map(|i| {
            string(spielorders_list_field(
                &list,
                i,
                SPIELORDERS_VENDOR_FIELD_NAME,
            ))
        })
        .collect())
}

#[test]
fn a_search_confined_to_one_fact_matches_only_in_that_fact() {
    let root = initialized("confined-search");
    let package = open(&root.0);
    let mut draft = None;
    spielorders_package_new_vendor_draft(&package, &mut draft, None);
    let mut draft = draft.unwrap();
    set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, "Tarnwick");
    set(
        &mut draft,
        SPIELORDERS_VENDOR_FIELD_PHONE,
        "+1 555 0123 extn 7",
    );
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
    commit(&plan_draft(&draft).unwrap());
    let laser = created_order(&root.0, "Laser components", &[("Mirror", "2", "10")]);
    created_order(&root.0, "Mirror mounts", &[("Lens", "1", "84.5")]);
    let package = open(&root.0);
    let mut order = order_draft(&read_order(&package, &laser));
    set(&mut order, SPIELORDERS_ORDER_FIELD_VENDOR, "Tarnwick");
    commit(&plan_draft(&order).unwrap());
    let damaged = created_order(&root.0, "Breadboard", &[]);
    fs::write(damaged.join(".spielorders/order.json"), "{").unwrap();
    let package = open(&root.0);
    let every = SpielordersConstant(-1);

    let offered: Vec<_> = (0..spielorders_order_search_field_count())
        .map(|index| spielorders_order_search_field(index))
        .collect();
    assert_eq!(
        offered[0], SPIELORDERS_ORDER_FIELD_TITLE,
        "menu order starts with Title"
    );
    for field in [
        SPIELORDERS_ORDER_FIELD_VENDOR,
        SPIELORDERS_ITEM_FIELD_DESCRIPTION,
        SPIELORDERS_ITEM_FIELD_UNIT_PRICE,
    ] {
        assert!(offered.contains(&field));
    }
    for field in [
        SPIELORDERS_ORDER_FIELD_TOTAL,
        SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME,
        SPIELORDERS_ITEM_FIELD_TOTAL,
        SPIELORDERS_VENDOR_FIELD_PHONE,
    ] {
        assert!(
            !offered.contains(&field),
            "{field:?} is no searchable Order fact"
        );
        assert_eq!(
            search_orders(&package, "mirror", field, 0),
            Err(SPIELORDERS_RESULT_BAD_ARGUMENT)
        );
    }
    assert_eq!(
        spielorders_order_search_field(spielorders_order_search_field_count()),
        every
    );
    for field in offered {
        assert!(search_orders(&package, "x", field, 0).is_ok(), "{field:?}");
    }

    assert_eq!(
        search_orders(&package, "mirror", every, 0).unwrap(),
        ["Laser components", "Mirror mounts"]
    );
    assert_eq!(
        search_orders(&package, "mirror", SPIELORDERS_ORDER_FIELD_TITLE, 0).unwrap(),
        ["Mirror mounts"]
    );
    assert_eq!(
        search_orders(&package, "mirror", SPIELORDERS_ITEM_FIELD_DESCRIPTION, 0).unwrap(),
        ["Laser components"]
    );
    assert!(
        search_orders(
            &package,
            "mirror",
            SPIELORDERS_ORDER_FIELD_TITLE,
            SPIELORDERS_LIST_CASE_SENSITIVE
        )
        .unwrap()
        .is_empty(),
        "Match Case applies within the fact"
    );
    assert_eq!(
        search_orders(&package, "84.50", SPIELORDERS_ITEM_FIELD_UNIT_PRICE, 0).unwrap(),
        ["Mirror mounts"],
        "a monetary fact keeps its rounded spelling"
    );
    assert!(
        search_orders(&package, "84.50", SPIELORDERS_ITEM_FIELD_QUANTITY, 0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        search_orders(&package, "tarn", SPIELORDERS_ORDER_FIELD_VENDOR, 0).unwrap(),
        ["Laser components"],
        "the Vendor fact is the referenced Vendor's Name"
    );
    assert!(
        search_orders(&package, "extn", SPIELORDERS_ORDER_FIELD_VENDOR, 0)
            .unwrap()
            .is_empty()
    );
    // A word no identity holds, since identities are hexadecimal.
    assert_eq!(
        search_orders(&package, "extn", every, 0).unwrap(),
        ["Laser components"],
        "every fact still reaches the Vendor's shared facts"
    );
    assert_eq!(
        search_orders(&package, "", SPIELORDERS_ORDER_FIELD_TITLE, 0)
            .unwrap()
            .len(),
        3,
        "an empty query lists the Damaged Order as every list does"
    );

    let offered: Vec<_> = (0..spielorders_vendor_search_field_count())
        .map(|index| spielorders_vendor_search_field(index))
        .collect();
    assert_eq!(offered[0], SPIELORDERS_VENDOR_FIELD_NAME);
    assert!(offered.contains(&SPIELORDERS_CONTACT_FIELD_NAME));
    assert!(!offered.contains(&SPIELORDERS_VENDOR_FIELD_HIDDEN));
    assert_eq!(
        search_vendors(&package, "grace", every).unwrap(),
        ["Tarnwick"]
    );
    assert_eq!(
        search_vendors(&package, "grace", SPIELORDERS_CONTACT_FIELD_NAME).unwrap(),
        ["Tarnwick"]
    );
    assert!(
        search_vendors(&package, "grace", SPIELORDERS_VENDOR_FIELD_NAME)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        search_vendors(&package, "grace", SPIELORDERS_ORDER_FIELD_TITLE),
        Err(SPIELORDERS_RESULT_BAD_ARGUMENT)
    );
}

#[test]
fn folder_contents_lists_orders_whose_facts_or_supporting_material_match() {
    let root = initialized("folder-contents");
    let laser = created_order(&root.0, "Laser components", &[]);
    let mounts = created_order(&root.0, "Mirror mounts", &[]);
    created_order(&root.0, "Breadboard", &[]);
    let quote = mounts.join("quote.pdf");
    fs::write(&quote, "quoted").unwrap();
    let package = open(&root.0);
    let search = searched(
        &package,
        &[(quote.as_path(), "pdf", "", "")],
        &[],
        SPIELORDERS_PROVIDER_STATE_COMPLETE,
    )
    .unwrap();
    let contents = |query: &str, flags: u32, material: Option<&SpielordersSearch>| {
        let query = c(query);
        let mut list = None;
        assert_eq!(
            spielorders_package_search_contents(
                &package,
                OptionalText::new(&query),
                flags,
                material,
                SPIELORDERS_ORDER_FIELD_TITLE,
                0,
                0,
                &mut list,
                None,
            ),
            SPIELORDERS_RESULT_OK
        );
        list.unwrap()
    };

    let found = contents("laser", 0, Some(&search));
    assert_eq!(titles(&found), ["Laser components", "Mirror mounts"]);
    assert_eq!(path(spielorders_list_path(&found, 0)), laser);
    assert_eq!(
        path(spielorders_list_path(&found, 1)),
        path(spielorders_search_order_directory(&search, 0)),
        "a row and its matched files name the Order Directory alike"
    );
    assert_eq!(
        titles(&contents(
            "LASER",
            SPIELORDERS_LIST_CASE_SENSITIVE,
            Some(&search)
        )),
        ["Mirror mounts"],
        "Match Case narrows the facts, never the Supporting Material"
    );
    assert_eq!(
        titles(&contents("laser", 0, None)),
        ["Laser components"],
        "with no Supporting Material search, the facts alone"
    );
    assert_eq!(
        titles(&contents("mirror", 0, Some(&search))),
        ["Mirror mounts"],
        "an Order matching both ways appears once"
    );
}

#[test]
fn an_item_note_is_drafted_read_back_and_searched_as_its_own_fact() {
    let root = initialized("item-notes");
    let laser = created_order(&root.0, "Laser components", &[("Patch cable", "2", "40")]);
    created_order(&root.0, "Mirror mounts", &[("Mount", "1", "84.5")]);
    let package = open(&root.0);
    let mut draft = order_draft(&read_order(&package, &laser));
    set_row(
        &mut draft,
        0,
        SPIELORDERS_ITEM_FIELD_NOTES,
        "Production notes: under 2 dB\nJacket 3 mm",
    );
    assert_eq!(
        string(spielorders_draft_row_field(
            &draft,
            0,
            SPIELORDERS_ITEM_FIELD_NOTES
        )),
        "Production notes: under 2 dB\nJacket 3 mm"
    );
    commit(&plan_draft(&draft).unwrap());

    let package = open(&root.0);
    let order = read_order(&package, &laser);
    assert_eq!(
        string(spielorders_order_item_field(
            &order,
            0,
            SPIELORDERS_ITEM_FIELD_NOTES
        )),
        "Production notes: under 2 dB\nJacket 3 mm"
    );
    assert_eq!(
        string(spielorders_order_item_field(
            &order,
            0,
            SPIELORDERS_ITEM_FIELD_TOTAL
        )),
        "80",
        "Notes never change a total"
    );
    assert!(
        (0..spielorders_order_search_field_count())
            .any(|index| spielorders_order_search_field(index) == SPIELORDERS_ITEM_FIELD_NOTES)
    );
    assert_eq!(
        search_orders(&package, "jacket", SPIELORDERS_ITEM_FIELD_NOTES, 0).unwrap(),
        ["Laser components"]
    );
    assert_eq!(
        search_orders(&package, "jacket", SpielordersConstant(-1), 0).unwrap(),
        ["Laser components"]
    );
    assert!(
        search_orders(&package, "jacket", SPIELORDERS_ITEM_FIELD_DESCRIPTION, 0)
            .unwrap()
            .is_empty()
    );

    let mut draft = order_draft(&order);
    set_row(&mut draft, 0, SPIELORDERS_ITEM_FIELD_NOTES, "");
    commit(&plan_draft(&draft).unwrap());
    let order = read_order(&open(&root.0), &laser);
    assert_eq!(
        string(spielorders_order_item_field(
            &order,
            0,
            SPIELORDERS_ITEM_FIELD_NOTES
        )),
        ""
    );
}

/// Creates an Order in `project` and `group`, introducing either where it is new.
fn classified_order(root: &Path, title: &str, project: &str, group: &str) -> PathBuf {
    let package = open(root);
    let mut draft = None;
    spielorders_package_new_order_draft(&package, &mut draft, None);
    let mut draft = draft.unwrap();
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, title);
    for (field, value) in [
        (SPIELORDERS_ORDER_FIELD_PROJECT, project),
        (SPIELORDERS_ORDER_FIELD_ORDER_GROUP, group),
    ] {
        set(&mut draft, field, value);
        if !value.is_empty() {
            assert_eq!(
                spielorders_draft_confirm(&mut draft, field, -1, None),
                SPIELORDERS_RESULT_OK
            );
        }
    }
    let report = commit(&plan_draft(&draft).unwrap());
    path(spielorders_report_final_path(&report))
}

fn project_groups(package: &SpielordersPackage, project: Option<&str>) -> Vec<(String, String)> {
    let project = project.map(c);
    let mut list = None;
    assert_eq!(
        spielorders_package_project_groups(
            package,
            project
                .as_deref()
                .map_or(OptionalText::null(), OptionalText::new),
            &mut list,
            None
        ),
        SPIELORDERS_RESULT_OK
    );
    let list = list.unwrap();
    (0..spielorders_list_count(&list))
        .map(|i| {
            (
                string(spielorders_list_field(
                    &list,
                    i,
                    SPIELORDERS_VALUE_FIELD_VALUE,
                )),
                string(spielorders_list_field(
                    &list,
                    i,
                    SPIELORDERS_VALUE_FIELD_USAGE_COUNT,
                )),
            )
        })
        .collect()
}

#[test]
fn an_order_group_belongs_to_the_project_of_the_orders_that_use_it() {
    let root = initialized("project-groups");
    let rebuild = classified_order(&root.0, "Chamber parts", "Telescope", "2026 Rebuild");
    classified_order(&root.0, "Vacuum pump", "Telescope", "Vacuum");
    let shared = classified_order(&root.0, "Gauges", "Weather station", "Vacuum");
    classified_order(&root.0, "Spare cables", "", "Loose");
    let package = open(&root.0);
    let pair = |value: &str, count: &str| (value.to_owned(), count.to_owned());

    assert_eq!(
        project_groups(&package, Some("Telescope")),
        [pair("2026 Rebuild", "1"), pair("Vacuum", "1")]
    );
    assert_eq!(
        project_groups(&package, Some(" teleSCOPE ")),
        project_groups(&package, Some("Telescope")),
        "a Project is named as reuse names it"
    );
    assert_eq!(
        project_groups(&package, Some("Weather station")),
        [pair("Vacuum", "1")],
        "the same name under another Project is its own group"
    );
    assert_eq!(project_groups(&package, None), [pair("Loose", "1")]);
    assert_eq!(project_groups(&package, Some("")), [pair("Loose", "1")]);
    assert!(project_groups(&package, Some("Unknown")).is_empty());

    // Moving an Order to a Project without its group clears the group; a Project that
    // already has a group of that name keeps it.
    let mut moved = order_draft(&read_order(&package, &rebuild));
    set(
        &mut moved,
        SPIELORDERS_ORDER_FIELD_PROJECT,
        "Weather station",
    );
    commit(&plan_draft(&moved).unwrap());
    let package = open(&root.0);
    let order = read_order(&package, &rebuild);
    assert_eq!(
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_PROJECT
        )),
        "Weather station"
    );
    assert_eq!(
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_ORDER_GROUP
        )),
        ""
    );
    let mut kept = order_draft(&read_order(&package, &shared));
    set(&mut kept, SPIELORDERS_ORDER_FIELD_PROJECT, "Telescope");
    commit(&plan_draft(&kept).unwrap());
    let package = open(&root.0);
    assert_eq!(
        string(spielorders_order_field(
            &read_order(&package, &shared),
            SPIELORDERS_ORDER_FIELD_ORDER_GROUP
        )),
        "Vacuum"
    );
    assert_eq!(
        project_groups(&package, Some("Telescope")),
        [pair("2026 Rebuild", "0"), pair("Vacuum", "2")],
        "a group the Package keeps stays when no Order uses it"
    );

    // A group named in the edit is judged in the new Project, and a new one there needs
    // confirming as any new value does.
    let mut named = order_draft(&read_order(&package, &shared));
    set(
        &mut named,
        SPIELORDERS_ORDER_FIELD_PROJECT,
        "Weather station",
    );
    set(&mut named, SPIELORDERS_ORDER_FIELD_ORDER_GROUP, "Optics");
    let Err(refused) = plan_draft(&named) else {
        panic!("a new group in the new Project needs confirming");
    };
    assert_eq!(refused[0].0, "unconfirmed_reusable_value");
}

#[test]
fn order_group_spelling_is_independent_of_project_names() {
    let root = initialized("group-spelling");
    let directory = classified_order(&root.0, "Lenses", "Assembly", "optics");
    classified_order(&root.0, "Cables", "OPTICS", "");

    let package = open(&root.0);
    let mut draft = order_draft(&read_order(&package, &directory));
    set(
        &mut draft,
        SPIELORDERS_ORDER_FIELD_TITLE,
        "Replacement lenses",
    );
    commit(&plan_draft(&draft).unwrap());

    let order = read_order(&open(&root.0), &directory);
    assert_eq!(
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_TITLE
        )),
        "Replacement lenses"
    );
    assert_eq!(
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_ORDER_GROUP
        )),
        "optics"
    );
}

#[test]
fn an_order_edit_checks_current_group_spelling_only_in_its_project() {
    for (project, expected) in [
        ("Assembly", SPIELORDERS_RESULT_REFUSED),
        ("Shipping", SPIELORDERS_RESULT_OK),
    ] {
        let root = initialized("group-stale-spelling");
        let directory = classified_order(&root.0, "Lenses", "Assembly", "");
        let package = open(&root.0);
        let mut draft = order_draft(&read_order(&package, &directory));
        set(&mut draft, SPIELORDERS_ORDER_FIELD_ORDER_GROUP, "optics");
        assert_eq!(
            spielorders_draft_confirm(&mut draft, SPIELORDERS_ORDER_FIELD_ORDER_GROUP, -1, None),
            SPIELORDERS_RESULT_OK
        );
        let plan = plan_draft(&draft).unwrap();

        classified_order(&root.0, "Mirrors", project, "Optics");
        let before = tree(&root.0);
        let mut report = None;
        let mut outcome = None;
        assert_eq!(
            spielorders_plan_commit(&plan, &mut report, Some(&mut outcome)),
            expected
        );
        let expected_group = if expected == SPIELORDERS_RESULT_REFUSED {
            assert!(report.is_none());
            assert_eq!(codes(&outcome), ["plan_evidence_unconfirmed"]);
            assert_eq!(
                string(spielorders_entries_message(outcome.as_deref(), 0)),
                "Plan evidence unconfirmed: order_group"
            );
            assert_eq!(tree(&root.0), before, "a stale plan writes nothing");
            ""
        } else {
            assert!(report.is_some());
            "optics"
        };

        let order = read_order(&open(&root.0), &directory);
        assert_eq!(
            string(spielorders_order_field(
                &order,
                SPIELORDERS_ORDER_FIELD_ORDER_GROUP
            )),
            expected_group
        );
    }
}

fn vendor_named(package: &SpielordersPackage, name: &str) -> Option<Box<SpielordersVendor>> {
    let name = c(name);
    let mut vendor = None;
    spielorders_package_read_vendor(
        package,
        SPIELORDERS_VENDOR_SELECTOR_NAME,
        Text::new(&name),
        0,
        &mut vendor,
        None,
    );
    vendor
}

#[test]
fn only_a_vendor_no_order_references_is_deleted() {
    let root = initialized("vendor-delete");
    for name in ["Tarnwick", "Elderglass Optics"] {
        let package = open(&root.0);
        let mut draft = None;
        spielorders_package_new_vendor_draft(&package, &mut draft, None);
        let mut draft = draft.unwrap();
        set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, name);
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
        commit(&plan_draft(&draft).unwrap());
    }
    let mut orders = Vec::new();
    for title in ["Lenses", "Mounts"] {
        let directory = created_order(&root.0, title, &[]);
        let package = open(&root.0);
        let mut draft = order_draft(&read_order(&package, &directory));
        set(&mut draft, SPIELORDERS_ORDER_FIELD_VENDOR, "Tarnwick");
        let report = commit(&plan_draft(&draft).unwrap());
        orders.push(path(spielorders_report_final_path(&report)));
    }
    let package = open(&root.0);

    let referenced = vendor_named(&package, "Tarnwick").unwrap();
    let mut plan = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_vendor_plan_delete(&referenced, 0, &mut plan, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(plan.is_none());
    let entries = outcome.as_deref();
    let listed: Vec<_> = (0..spielorders_entries_count(entries))
        .map(|i| {
            (
                string(spielorders_constant_name(spielorders_entries_code(
                    entries, i,
                ))),
                string(spielorders_entries_path(entries, i)),
            )
        })
        .collect();
    let relative = |order: &Path| {
        order
            .strip_prefix(&root.0)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(
        listed,
        orders
            .iter()
            .map(|order| ("vendor_referenced".to_owned(), relative(order)))
            .collect::<Vec<_>>(),
        "every referencing Order is listed"
    );

    let unused = vendor_named(&package, "Elderglass Optics").unwrap();
    let mut plan = None;
    assert_eq!(
        spielorders_vendor_plan_delete(&unused, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    let plan = plan.unwrap();
    assert_eq!(spielorders_plan_change_count(&plan), 1);
    assert_eq!(
        spielorders_plan_change_kind(&plan, 0),
        SPIELORDERS_CHANGE_RECORD_REMOVED
    );
    assert_eq!(
        string(spielorders_plan_change_before(&plan, 0)),
        "Elderglass Optics"
    );
    assert!(
        vendor_named(&open(&root.0), "Elderglass Optics").is_some(),
        "planning writes nothing"
    );
    let report = commit(&plan);
    assert!(spielorders_report_vendor(&report).is_none());

    let package = open(&root.0);
    assert!(vendor_named(&package, "Elderglass Optics").is_none());
    assert!(vendor_named(&package, "Tarnwick").is_some());
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
    assert_eq!(spielorders_list_count(&all.unwrap()), 1);
}

#[test]
fn a_vendor_deletion_names_the_current_record_of_the_selected_identity() {
    let root = initialized("vendor-delete-identity");
    let package = open(&root.0);
    let mut draft = new_draft(&package, spielorders_package_new_vendor_draft);
    set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, "Acme Parts");
    let report = commit(&plan_draft(&draft).unwrap());
    let original = spielorders_report_vendor(&report).unwrap();

    let mut draft = None;
    assert_eq!(
        spielorders_vendor_draft(&original, &mut draft),
        SPIELORDERS_RESULT_OK
    );
    let mut draft = draft.unwrap();
    set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, "Acme Components");
    commit(&plan_draft(&draft).unwrap());

    let package = open(&root.0);
    let mut draft = new_draft(&package, spielorders_package_new_vendor_draft);
    set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, "Acme Parts");
    commit(&plan_draft(&draft).unwrap());

    let mut plan = None;
    assert_eq!(
        spielorders_vendor_plan_delete(&original, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    let plan = plan.unwrap();
    assert_eq!(spielorders_plan_change_count(&plan), 1);
    assert_eq!(
        spielorders_plan_change_kind(&plan, 0),
        SPIELORDERS_CHANGE_RECORD_REMOVED
    );
    assert_eq!(
        string(spielorders_plan_change_before(&plan, 0)),
        "Acme Components",
        "the plan names the current record, not the stale handle's Name"
    );
    commit(&plan);

    let package = open(&root.0);
    assert!(vendor_named(&package, "Acme Components").is_none());
    assert!(
        vendor_named(&package, "Acme Parts").is_some(),
        "the new Vendor reusing the old Name has a different identity"
    );
}

/// Removes the Order at `directory` and answers where it went.
fn removed(root: &Path, directory: &Path) -> PathBuf {
    let order = read_order(&open(root), directory);
    let mut plan = None;
    assert_eq!(
        spielorders_order_plan_remove(&order, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    path(spielorders_report_final_path(&commit(&plan.unwrap())))
}

fn plan_restore(
    root: &Path,
    removed: &Path,
) -> Result<Box<SpielordersPlan>, Vec<(String, String)>> {
    let package = open(root);
    let removed = c_path(removed);
    let mut plan = None;
    let mut outcome = None;
    match spielorders_package_plan_restore_order(
        &package,
        PathBytes::new(&removed),
        0,
        &mut plan,
        Some(&mut outcome),
    ) {
        SPIELORDERS_RESULT_OK => Ok(plan.unwrap()),
        _ => {
            let entries = outcome.as_deref();
            Err((0..spielorders_entries_count(entries))
                .map(|i| {
                    (
                        string(spielorders_constant_name(spielorders_entries_code(
                            entries, i,
                        ))),
                        string(spielorders_entries_path(entries, i)),
                    )
                })
                .collect())
        }
    }
}

#[test]
fn a_removed_order_is_restored_under_its_name_with_its_material() {
    let root = initialized("restore");
    let directory = created_order(&root.0, "Laser components", &[("Lens", "2", "10")]);
    let id = string(spielorders_order_field(
        &read_order(&open(&root.0), &directory),
        SPIELORDERS_ORDER_FIELD_ORDER_ID,
    ));
    fs::write(directory.join("quote.pdf"), "quoted").unwrap();
    let gone = removed(&root.0, &directory);
    assert!(!directory.exists());

    let plan = plan_restore(&root.0, &gone).unwrap();
    assert_eq!(string(spielorders_plan_restored_order_id(&plan)), id);
    assert!(gone.exists(), "planning moves nothing");
    let report = commit(&plan);
    assert_eq!(path(spielorders_report_final_path(&report)), directory);
    let restored = spielorders_report_order(&report).expect("the restored Order is read back");
    assert_eq!(
        string(spielorders_order_field(
            &restored,
            SPIELORDERS_ORDER_FIELD_TITLE
        )),
        "Laser components"
    );
    assert!(!gone.exists());
    assert_eq!(
        fs::read_to_string(directory.join("quote.pdf")).unwrap(),
        "quoted"
    );
    assert_eq!(
        titles(&orders(&open(&root.0), None, 0, SpielordersConstant(-1), 0)),
        ["Laser components"]
    );
}

#[test]
fn a_restore_plan_identifies_its_order_even_when_the_removed_path_was_reused() {
    for after_planning in [false, true] {
        let root = initialized("restore-subject-identity");
        let first = created_order(&root.0, "Lenses", &[]);
        let second = created_order(&root.0, "Mirrors", &[]);
        let package = open(&root.0);
        let identity = |directory: &Path| {
            string(spielorders_order_field(
                &read_order(&package, directory),
                SPIELORDERS_ORDER_FIELD_ORDER_ID,
            ))
        };
        let first_id = identity(&first);
        let second_id = identity(&second);
        let first = removed(&root.0, &first);
        let second = removed(&root.0, &second);
        let plan = after_planning.then(|| plan_restore(&root.0, &first).unwrap());
        fs::rename(&first, root.0.join("Saved original")).unwrap();
        fs::rename(&second, &first).unwrap();
        let before = tree(&root.0);
        if let Some(plan) = plan {
            assert_eq!(string(spielorders_plan_restored_order_id(&plan)), first_id);
            let mut report = None;
            assert_eq!(
                spielorders_plan_commit(&plan, &mut report, None),
                SPIELORDERS_RESULT_REFUSED,
                "the planned identity no longer stands at the removed path"
            );
            assert!(report.is_none());
        } else {
            let plan = plan_restore(&root.0, &first).unwrap();
            let planned_id = string(spielorders_plan_restored_order_id(&plan));
            assert_eq!(
                planned_id, second_id,
                "the plan identifies the actual subject"
            );
            assert_ne!(
                planned_id, first_id,
                "a caller can reject a stale Undo target"
            );
        }
        assert_eq!(
            tree(&root.0),
            before,
            "inspection and refusal write nothing"
        );
        let mut index = None;
        assert_eq!(
            spielorders_package_plan_rebuild_index(&open(&root.0), 0, &mut index, None),
            SPIELORDERS_RESULT_OK
        );
        assert_eq!(
            string(spielorders_plan_restored_order_id(&index.unwrap())),
            ""
        );
    }
}

#[test]
fn a_restore_refuses_what_is_not_its_removed_order_to_take_back() {
    let root = initialized("restore-refusals");
    let active = created_order(&root.0, "Mirror mounts", &[]);
    let directory = created_order(&root.0, "Laser components", &[]);
    let gone = removed(&root.0, &directory);
    let relative = |path: &Path| {
        path.strip_prefix(&root.0)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    };

    let code = |result: Result<Box<SpielordersPlan>, Vec<(String, String)>>| match result {
        Ok(_) => panic!("the restore must refuse"),
        Err(entries) => entries[0].clone(),
    };
    assert_eq!(code(plan_restore(&root.0, &active)).0, "not_direct_child");
    assert_eq!(
        code(plan_restore(&root.0, &gone.with_file_name("absent"))).0,
        "not_an_order_directory"
    );

    // Something now has the Order's name in the Orders Directory.
    fs::create_dir(&directory).unwrap();
    assert_eq!(
        code(plan_restore(&root.0, &gone)),
        ("destination_collision".to_owned(), relative(&directory))
    );
    fs::remove_dir(&directory).unwrap();

    // A copy of it came back under another name, carrying its identity.
    let copy = root.0.join("Orders/Copied by hand");
    fs::create_dir_all(copy.join(".spielorders")).unwrap();
    let record = fs::read(gone.join(".spielorders/order.json")).unwrap();
    fs::write(copy.join(".spielorders/order.json"), &record).unwrap();
    fs::write(copy.join("README.md"), "").unwrap();
    assert_eq!(
        code(plan_restore(&root.0, &gone)),
        ("order_identity_in_use".to_owned(), relative(&copy))
    );
    fs::remove_dir_all(&copy).unwrap();

    // A plan whose Order changed before its commit writes nothing.
    let plan = plan_restore(&root.0, &gone).unwrap();
    let edited = String::from_utf8(record)
        .unwrap()
        .replace("Laser components", "Laser parts");
    fs::write(gone.join(".spielorders/order.json"), edited).unwrap();
    let mut report = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_plan_commit(&plan, &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(gone.exists() && !directory.exists());
}

#[test]
fn a_restore_gives_the_order_back_its_locator_at_its_own_depth() {
    let directory = initialized_in(
        "restore-nested",
        OptionalPathBytes::new(c"Purchasing/Orders"),
    );
    let order = created_order(&directory.0, "Laser components", &[]);
    assert!(order.starts_with(directory.0.join("Purchasing/Orders")));
    let gone = removed(&directory.0, &order);
    let locator = |at: &Path| {
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(at.join(".spielorders/order.json")).unwrap()).unwrap();
        record["package_root"].as_str().unwrap().to_owned()
    };
    assert_eq!(
        locator(&gone),
        "../..",
        "removal set the Removed Orders depth"
    );

    let report = commit(&plan_restore(&directory.0, &gone).unwrap());
    assert_eq!(path(spielorders_report_final_path(&report)), order);
    assert_eq!(locator(&order), "../../..");
    assert!(
        spielorders_report_order(&report).is_some(),
        "the restored Order reads as healthy"
    );
}

#[test]
fn a_restore_refuses_an_identity_carried_by_a_damaged_order() {
    for after_planning in [false, true] {
        let root = initialized("restore-damaged-identity");
        let directory = created_order(&root.0, "Laser components", &[]);
        let gone = removed(&root.0, &directory);
        let plan = after_planning.then(|| plan_restore(&root.0, &gone).unwrap());
        let damaged = root.0.join("Orders/Damaged copy");
        fs::create_dir_all(damaged.join(".spielorders")).unwrap();
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(gone.join(".spielorders/order.json")).unwrap())
                .unwrap();
        record["status"] = "not a status".into();
        fs::write(
            damaged.join(".spielorders/order.json"),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        let before = tree(&root.0);
        if let Some(plan) = plan {
            let mut report = None;
            assert_eq!(
                spielorders_plan_commit(&plan, &mut report, None),
                SPIELORDERS_RESULT_REFUSED,
                "an identity taken after planning also refuses"
            );
            assert!(report.is_none());
        } else {
            let entries = match plan_restore(&root.0, &gone) {
                Ok(_) => panic!("a damaged Order still carries its readable identity"),
                Err(entries) => entries,
            };
            assert_eq!(
                entries[0],
                (
                    "order_identity_in_use".to_owned(),
                    "Orders/Damaged copy".to_owned()
                )
            );
        }
        assert_eq!(tree(&root.0), before, "a refusal changes nothing");
    }
}

#[test]
fn a_restore_refuses_a_vendor_reference_that_no_longer_resolves() {
    for after_planning in [false, true] {
        let root = initialized("restore-deleted-vendor");
        let package = open(&root.0);
        let mut vendor = new_draft(&package, spielorders_package_new_vendor_draft);
        set(&mut vendor, SPIELORDERS_VENDOR_FIELD_NAME, "Acme Parts");
        commit(&plan_draft(&vendor).unwrap());
        let directory = created_order(&root.0, "Laser components", &[]);
        let mut order = order_draft(&read_order(&open(&root.0), &directory));
        set(&mut order, SPIELORDERS_ORDER_FIELD_VENDOR, "Acme Parts");
        commit(&plan_draft(&order).unwrap());
        let gone = removed(&root.0, &directory);
        let restore = after_planning.then(|| plan_restore(&root.0, &gone).unwrap());
        // SpielOrders refuses this deletion, so the Vendor is deleted by hand.
        let vendors = root.0.join(".spielorders/vendors.json");
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&vendors).unwrap()).unwrap();
        record["vendors"] = serde_json::json!([]);
        fs::write(&vendors, serde_json::to_vec(&record).unwrap()).unwrap();
        let before = tree(&root.0);
        if let Some(restore) = restore {
            let mut report = None;
            assert_eq!(
                spielorders_plan_commit(&restore, &mut report, None),
                SPIELORDERS_RESULT_REFUSED
            );
            assert!(report.is_none());
        } else {
            let entries = match plan_restore(&root.0, &gone) {
                Ok(_) => panic!("a removed Order cannot bring back an unresolved Vendor reference"),
                Err(entries) => entries,
            };
            assert_eq!(entries[0].0, "unresolved_vendor");
        }
        assert_eq!(tree(&root.0), before, "no move, record or index write");
    }
}

#[test]
fn a_restore_refuses_when_the_configured_orders_directory_changes() {
    let root = initialized("restore-changed-destination");
    let directory = created_order(&root.0, "Laser components", &[]);
    let gone = removed(&root.0, &directory);
    let plan = plan_restore(&root.0, &gone).unwrap();
    fs::create_dir(root.0.join("NewOrders")).unwrap();
    fs::write(
        root.0.join(".spielorders/config.toml"),
        "orders_path = \"NewOrders\"\n",
    )
    .unwrap();
    let before = tree(&root.0);
    let mut report = None;
    assert_eq!(
        spielorders_plan_commit(&plan, &mut report, None),
        SPIELORDERS_RESULT_REFUSED,
        "the old directory is no longer the active Orders Directory"
    );
    assert!(report.is_none());
    assert_eq!(tree(&root.0), before, "a refusal changes nothing");
}

#[test]
fn a_restore_never_follows_linked_order_metadata() {
    for linked_directory in [false, true] {
        for after_planning in [false, true] {
            let root = initialized_in(
                "restore-linked-metadata",
                OptionalPathBytes::new(c"Purchasing/Orders"),
            );
            let external = Directory::new("restore-external-metadata");
            let directory = created_order(&root.0, "Laser components", &[]);
            let gone = removed(&root.0, &directory);
            let plan = after_planning.then(|| plan_restore(&root.0, &gone).unwrap());
            let metadata = gone.join(".spielorders");
            let linked = if linked_directory {
                metadata
            } else {
                metadata.join("order.json")
            };
            let target = external.0.join("metadata");
            fs::rename(&linked, &target).unwrap();
            std::os::unix::fs::symlink(&target, &linked).unwrap();
            let before = tree(&root.0);
            let outside = tree(&external.0);
            if let Some(plan) = plan {
                let mut report = None;
                assert_eq!(
                    spielorders_plan_commit(&plan, &mut report, None),
                    SPIELORDERS_RESULT_REFUSED
                );
                assert!(report.is_none());
            } else {
                assert!(
                    plan_restore(&root.0, &gone).is_err(),
                    "a linked managed component is not this Package's writable record"
                );
            }
            assert_eq!(tree(&root.0), before, "no move or index write");
            assert_eq!(tree(&external.0), outside, "no outside bytes change");
        }
    }
}

#[test]
fn order_mutations_never_write_through_a_linked_metadata_directory() {
    for removal in [false, true] {
        for after_planning in [false, true] {
            let root = initialized("order-linked-metadata");
            let external = Directory::new("order-external-metadata");
            let directory = created_order(&root.0, "Laser components", &[]);
            let order = read_order(&open(&root.0), &directory);
            let mut draft = order_draft(&order);
            spielorders_draft_set(
                &mut draft,
                SPIELORDERS_ORDER_FIELD_TITLE,
                Text::new(c"Laser parts"),
                None,
            );
            let make_plan = || {
                let mut plan = None;
                let result = if removal {
                    spielorders_order_plan_remove(&order, 0, &mut plan, None)
                } else {
                    spielorders_draft_plan(&draft, 0, &mut plan, None)
                };
                (result, plan)
            };
            let plan = after_planning.then(|| {
                let (result, plan) = make_plan();
                assert_eq!(result, SPIELORDERS_RESULT_OK);
                plan.unwrap()
            });
            let metadata = directory.join(".spielorders");
            let target = external.0.join("metadata");
            fs::rename(&metadata, &target).unwrap();
            std::os::unix::fs::symlink(&target, &metadata).unwrap();
            let before = tree(&root.0);
            let outside = tree(&external.0);
            if let Some(plan) = plan {
                let mut report = None;
                assert_eq!(
                    spielorders_plan_commit(&plan, &mut report, None),
                    SPIELORDERS_RESULT_REFUSED
                );
            } else {
                assert_eq!(make_plan().0, SPIELORDERS_RESULT_REFUSED);
            }
            assert_eq!(tree(&root.0), before, "no move or index write");
            assert_eq!(
                tree(&external.0),
                outside,
                "outside metadata stays untouched"
            );
        }
    }
}

#[test]
fn a_refused_restore_preserves_the_removed_record() {
    use std::os::unix::fs::PermissionsExt;
    for collision in [false, true] {
        let root = initialized_in(
            "restore-failed-move",
            OptionalPathBytes::new(c"Purchasing/Orders"),
        );
        let directory = created_order(&root.0, "Laser components", &[]);
        let gone = removed(&root.0, &directory);
        let plan = plan_restore(&root.0, &gone).unwrap();
        let orders = directory.parent().unwrap();
        if collision {
            fs::create_dir(&directory).unwrap();
        } else {
            fs::set_permissions(orders, fs::Permissions::from_mode(0o500)).unwrap();
        }
        let before = tree(&root.0);
        let mut report = None;
        let mut outcome = None;
        let result = spielorders_plan_commit(&plan, &mut report, Some(&mut outcome));
        fs::set_permissions(orders, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(result, SPIELORDERS_RESULT_REFUSED);
        assert!(report.is_none());
        assert!(
            !codes(&outcome).contains(&"commit_failed_cleanup_incomplete".to_owned()),
            "the original record can be restored"
        );
        assert_eq!(
            tree(&root.0),
            before,
            "the removed record keeps its original bytes"
        );
    }
}

#[test]
fn a_refresh_never_replaces_guidance_edited_after_planning() {
    let directory = Directory::new("agent-refresh-stale");
    let root = &directory.0;
    let skill = root.join(".agents/skills/spielorders");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "old skill\n").unwrap();
    let (result, plan, _) = provision(root, 0);
    assert_eq!(result, SPIELORDERS_RESULT_OK);
    let plan = plan.unwrap();

    fs::write(skill.join("SKILL.md"), "edited meanwhile\n").unwrap();
    let mut report = None;
    assert_eq!(
        spielorders_plan_commit(&plan, &mut report, None),
        SPIELORDERS_RESULT_REFUSED
    );
    assert_eq!(
        fs::read_to_string(skill.join("SKILL.md")).unwrap(),
        "edited meanwhile\n"
    );
}

#[test]
fn a_refused_guidance_refresh_restores_earlier_replacements() {
    let directory = Directory::new("agent-refresh-rollback");
    let skill = directory.0.join(".agents/skills/spielorders");
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(skill.join("SKILL.md"), "local skill\n").unwrap();
    let second = skill.join("references/packages-and-adoption.md");
    fs::write(&second, "local reference\n").unwrap();
    let (result, plan, _) = provision(&directory.0, 0);
    assert_eq!(result, SPIELORDERS_RESULT_OK);
    fs::write(&second, "edited meanwhile\n").unwrap();
    let before = tree(&directory.0);
    let mut report = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_plan_commit(&plan.unwrap(), &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(report.is_none());
    assert_eq!(codes(&outcome), ["commit_failed"]);
    assert_eq!(
        tree(&directory.0),
        before,
        "local files and later work survive"
    );
}

/// Merging values through the interface names them as an array, plans how many Orders
/// change, and leaves every one of them on the kept spelling.
#[test]
fn merging_values_counts_and_changes_every_order_using_them() {
    let root = initialized("merge-values");
    let cell = classified_order(&root.0, "Cell", "AbC", "");
    let oven = classified_order(&root.0, "Oven", "Ab_C", "");
    let lamp = classified_order(&root.0, "Lamp", "ab_c", "");
    let package = open(&root.0);
    let (abc, ab_c, keep) = (c("AbC"), c("Ab_C"), c("AbC"));
    let values = [Text::new(&abc), Text::new(&ab_c)];
    let mut plan = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_package_plan_merge_values(
            &package,
            SPIELORDERS_ORDER_FIELD_PROJECT,
            OptionalText::null(),
            std::ptr::NonNull::new(values.as_ptr().cast_mut()),
            2,
            Text::new(&keep),
            0,
            &mut plan,
            Some(&mut outcome),
        ),
        SPIELORDERS_RESULT_OK,
        "{:?}",
        located(&outcome)
    );
    let plan = plan.unwrap();
    assert_eq!(spielorders_plan_order_count(&plan), 2);
    assert_eq!(
        spielorders_report_kind(&commit(&plan)),
        SPIELORDERS_REPORT_COMMITTED
    );

    let package = open(&root.0);
    for directory in [&cell, &oven, &lamp] {
        assert_eq!(
            string(spielorders_order_field(
                &read_order(&package, directory),
                SPIELORDERS_ORDER_FIELD_PROJECT
            )),
            "AbC"
        );
    }

    let gone = c("Ab_C");
    let mut refused = None;
    let mut outcome = None;
    assert_ne!(
        spielorders_package_plan_delete_value(
            &package,
            SPIELORDERS_ORDER_FIELD_PROJECT,
            OptionalText::null(),
            Text::new(&gone),
            OptionalText::null(),
            0,
            &mut refused,
            Some(&mut outcome),
        ),
        SPIELORDERS_RESULT_OK
    );
    assert_eq!(codes(&outcome), ["unknown_value"]);
}

/// An Order's aligned name is the directory name aligning it gives, read without planning
/// the alignment: its own name until a fact it projects changes.
#[test]
fn an_orders_aligned_name_tells_whether_its_directory_has_drifted() {
    let root = initialized("aligned-name");
    let directory = created_order(&root.0, "Laser components", &[]);
    let package = open(&root.0);
    let order = read_order(&package, &directory);
    assert_eq!(
        string(spielorders_order_aligned_name(&order)),
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME
        ))
    );

    let mut draft = order_draft(&order);
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Mirror mounts");
    commit(&plan_draft(&draft).unwrap());
    let package = open(&root.0);
    let order = read_order(&package, &directory);
    let aligned = string(spielorders_order_aligned_name(&order));
    assert!(aligned.ends_with("_Mirror_mounts"), "{aligned}");
    assert_ne!(
        aligned,
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME
        ))
    );
}

#[test]
fn an_order_save_refuses_a_value_deleted_after_planning() {
    for field in [
        SPIELORDERS_ORDER_FIELD_ORDERER,
        SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
        SPIELORDERS_ORDER_FIELD_PROJECT,
        SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
    ] {
        let root = initialized("deleted-planned-value");
        let directory = created_order(&root.0, "Lenses", &[]);
        let package = open(&root.0);
        let value = c("Ada");
        let mut addition = None;
        assert_eq!(
            spielorders_package_plan_add_value(
                &package,
                field,
                OptionalText::null(),
                Text::new(&value),
                0,
                &mut addition,
                None,
            ),
            SPIELORDERS_RESULT_OK
        );
        commit(&addition.unwrap());
        let order = read_order(&package, &directory);
        let mut draft = order_draft(&order);
        set(&mut draft, field, "Ada");
        let plan = plan_draft(&draft).unwrap();
        let mut deletion = None;
        assert_eq!(
            spielorders_package_plan_delete_value(
                &package,
                field,
                OptionalText::null(),
                Text::new(&value),
                OptionalText::null(),
                0,
                &mut deletion,
                None,
            ),
            SPIELORDERS_RESULT_OK
        );
        commit(&deletion.unwrap());
        let before = tree(&root.0);
        let mut report = None;
        let mut outcome = None;
        assert_eq!(
            spielorders_plan_commit(&plan, &mut report, Some(&mut outcome)),
            SPIELORDERS_RESULT_REFUSED,
            "a deleted field value must not be recreated by a stale plan"
        );
        assert!(report.is_none());
        assert_eq!(codes(&outcome), ["plan_evidence_unconfirmed"]);
        let name = string(spielorders_constant_name(field));
        assert_eq!(
            string(spielorders_entries_message(outcome.as_deref(), 0)),
            format!("Plan evidence unconfirmed: {name}"),
            "the refusal names the value that moved"
        );
        assert_eq!(tree(&root.0), before);
    }
}

/// The kept Vendor takes precedence: a merged Contact it already has by Name only fills
/// that Contact's missing facts, and every other one is added after its own, in order.
#[test]
fn a_vendor_merge_adds_only_the_contacts_the_kept_vendor_lacks() {
    let root = initialized("merge-contact-rows");
    let package = open(&root.0);
    let vendor = |name: &str, contacts: &[&str]| {
        let mut draft = new_draft(&package, spielorders_package_new_vendor_draft);
        set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, name);
        for (row, name) in contacts.iter().enumerate() {
            let row = i64::try_from(row).unwrap();
            assert_eq!(
                spielorders_draft_row_insert(&mut draft, row, None),
                SPIELORDERS_RESULT_OK
            );
            set_row(&mut draft, row, SPIELORDERS_CONTACT_FIELD_NAME, name);
        }
        let report = commit(&plan_draft(&draft).unwrap());
        let vendor = spielorders_report_vendor(&report).unwrap();
        string(spielorders_vendor_field(
            &vendor,
            SPIELORDERS_VENDOR_FIELD_VENDOR_ID,
        ))
    };
    let kept = c(&vendor("First supplier", &["Accounts"]));
    let other = c(&vendor(
        "Second supplier",
        &["Accounts", "Accounts", "Sales"],
    ));
    let ids = [Text::new(&other)];
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_merge_vendors(
            &package,
            std::ptr::NonNull::new(ids.as_ptr().cast_mut()),
            1,
            Text::new(&kept),
            0,
            &mut plan,
            None,
        ),
        SPIELORDERS_RESULT_OK
    );
    let report = commit(&plan.unwrap());
    let saved = spielorders_report_vendor(&report).expect("the merged Vendor is returned");
    assert_eq!(spielorders_vendor_contact_count(&saved), 2);
    let package = open(&root.0);
    let mut vendor = None;
    assert_eq!(
        spielorders_package_read_vendor(
            &package,
            SPIELORDERS_VENDOR_SELECTOR_ID,
            Text::new(&kept),
            0,
            &mut vendor,
            None,
        ),
        SPIELORDERS_RESULT_OK
    );
    let vendor = vendor.unwrap();
    let contacts = (0..spielorders_vendor_contact_count(&vendor))
        .map(|row| {
            string(spielorders_vendor_contact_field(
                &vendor,
                row,
                SPIELORDERS_CONTACT_FIELD_NAME,
            ))
        })
        .collect::<Vec<_>>();
    assert_eq!(contacts, ["Accounts", "Sales"]);
}

#[test]
fn adding_a_value_with_an_order_is_one_canonical_change() {
    use std::os::unix::fs::PermissionsExt;
    let mut outcomes = Vec::new();
    for route in ["create", "edit", "adopt"] {
        let root = initialized("atomic-order-value");
        let package = open(&root.0);
        let mut draft = match route {
            "create" => new_draft(&package, spielorders_package_new_order_draft),
            "edit" => {
                let directory = created_order(&root.0, "Original lenses", &[]);
                order_draft(&read_order(&package, &directory))
            }
            _ => {
                let directory = root.0.join("Orders/Legacy");
                fs::create_dir(&directory).unwrap();
                fs::write(directory.join("quote.txt"), "Original supporting text").unwrap();
                let directory = c_path(&directory);
                let mut draft = None;
                assert_eq!(
                    spielorders_package_adoption_draft(
                        &package,
                        PathBytes::new(&directory),
                        0,
                        &mut draft,
                        None,
                    ),
                    SPIELORDERS_RESULT_OK
                );
                let mut draft = draft.unwrap();
                set(
                    &mut draft,
                    SPIELORDERS_ORDER_FIELD_DATE_ORDERED,
                    "2026-11-13",
                );
                set(&mut draft, SPIELORDERS_ORDER_FIELD_STATUS, "ordered");
                draft
            }
        };
        set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Lenses");
        set(&mut draft, SPIELORDERS_ORDER_FIELD_ORDERER, "Ada");
        assert_eq!(
            spielorders_draft_confirm(&mut draft, SPIELORDERS_ORDER_FIELD_ORDERER, -1, None),
            SPIELORDERS_RESULT_OK
        );
        let plan = plan_draft(&draft).unwrap();
        let metadata = root.0.join(".spielorders");
        fs::set_permissions(&metadata, fs::Permissions::from_mode(0o555)).unwrap();
        let before = tree(&root.0);
        let mut report = None;
        let mut outcome = None;
        let result = spielorders_plan_commit(&plan, &mut report, Some(&mut outcome));
        fs::set_permissions(&metadata, fs::Permissions::from_mode(0o755)).unwrap();
        outcomes.push((route, result, report.is_none(), tree(&root.0) == before));
    }
    assert_eq!(
        outcomes,
        [
            ("create", SPIELORDERS_RESULT_REFUSED, true, true),
            ("edit", SPIELORDERS_RESULT_REFUSED, true, true),
            ("adopt", SPIELORDERS_RESULT_REFUSED, true, true),
        ]
    );
}

#[test]
fn value_change_reports_retain_their_affected_scope_after_another_commit() {
    let root = initialized("value-result-snapshot");
    let package = open(&root.0);
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_add_value(
            &package,
            SPIELORDERS_ORDER_FIELD_ORDERER,
            OptionalText::null(),
            Text::new(c"Ada"),
            0,
            &mut plan,
            None,
        ),
        SPIELORDERS_RESULT_OK
    );
    let added = commit(&plan.unwrap());
    let mut plan = None;
    assert_eq!(
        spielorders_package_plan_delete_value(
            &package,
            SPIELORDERS_ORDER_FIELD_ORDERER,
            OptionalText::null(),
            Text::new(c"Ada"),
            OptionalText::null(),
            0,
            &mut plan,
            None,
        ),
        SPIELORDERS_RESULT_OK
    );
    let deleted = commit(&plan.unwrap());
    let original = spielorders_report_values(&added).expect("the added values are returned");
    assert_eq!(spielorders_list_count(&original), 1);
    assert_eq!(
        string(spielorders_list_field(
            &original,
            0,
            SPIELORDERS_VALUE_FIELD_VALUE
        )),
        "Ada"
    );
    assert_eq!(
        spielorders_list_count(
            &spielorders_report_values(&deleted).expect("an empty scope is a result")
        ),
        0
    );
}

#[test]
fn destructive_vendor_changes_keep_references_in_damaged_orders_resolvable() {
    for damage in ["date", "shape", "duplicate"] {
        for action in ["delete", "clear", "replace", "merge"] {
            let root = initialized("damaged-vendor-reference");
            let package = open(&root.0);
            for name in ["First", "Kept", "Unrelated"] {
                let mut draft = new_draft(&package, spielorders_package_new_vendor_draft);
                set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, name);
                commit(&plan_draft(&draft).unwrap());
            }
            let directory = created_order(&root.0, "Damaged sample", &[]);
            let package = open(&root.0);
            let mut draft = order_draft(&read_order(&package, &directory));
            set(&mut draft, SPIELORDERS_ORDER_FIELD_VENDOR, "First");
            commit(&plan_draft(&draft).unwrap());
            let record = directory.join(".spielorders/order.json");
            let mut document: serde_json::Value =
                serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
            match damage {
                "date" => document["date_ordered"] = serde_json::json!("2026-02-30"),
                "shape" => document["title"] = serde_json::json!(42),
                "duplicate" => {
                    let duplicate = root.0.join("Orders/duplicate");
                    fs::create_dir_all(duplicate.join(".spielorders")).unwrap();
                    fs::copy(&record, duplicate.join(".spielorders/order.json")).unwrap();
                }
                _ => unreachable!(),
            }
            fs::write(&record, serde_json::to_vec(&document).unwrap()).unwrap();
            let before_order = fs::read(&record).unwrap();
            let vendors = root.0.join(".spielorders/vendors.json");
            let before_vendors = fs::read(&vendors).unwrap();
            let package = open(&root.0);
            let first = vendor_named(&package, "First").unwrap();
            let kept = vendor_named(&package, "Kept").unwrap();
            let first_id = c(&string(spielorders_vendor_field(
                &first,
                SPIELORDERS_VENDOR_FIELD_VENDOR_ID,
            )));
            let kept_id = c(&string(spielorders_vendor_field(
                &kept,
                SPIELORDERS_VENDOR_FIELD_VENDOR_ID,
            )));
            let mut plan = None;
            let mut outcome = None;
            let result = match action {
                "delete" => {
                    spielorders_vendor_plan_delete(&first, 0, &mut plan, Some(&mut outcome))
                }
                "clear" => spielorders_vendor_plan_delete_replacing(
                    &first,
                    OptionalText::new(c""),
                    0,
                    &mut plan,
                    Some(&mut outcome),
                ),
                "replace" => spielorders_vendor_plan_delete_replacing(
                    &first,
                    OptionalText::new(&kept_id),
                    0,
                    &mut plan,
                    Some(&mut outcome),
                ),
                "merge" => {
                    let mut ids = [Text::new(&first_id)];
                    spielorders_package_plan_merge_vendors(
                        &package,
                        std::ptr::NonNull::new(ids.as_mut_ptr()),
                        1,
                        Text::new(&kept_id),
                        0,
                        &mut plan,
                        Some(&mut outcome),
                    )
                }
                _ => unreachable!(),
            };
            assert_eq!(result, SPIELORDERS_RESULT_REFUSED, "{damage}: {action}");
            assert!(plan.is_none());
            assert!(
                codes(&outcome)
                    .iter()
                    .any(|code| code == "vendor_referenced")
            );
            assert_eq!(fs::read(&record).unwrap(), before_order);
            assert_eq!(fs::read(&vendors).unwrap(), before_vendors);

            // Damage in an Order that does not reference a deleted Vendor is no reason
            // to prevent that unrelated deletion.
            let unrelated = vendor_named(&package, "Unrelated").unwrap();
            let mut plan = None;
            assert_eq!(
                spielorders_vendor_plan_delete(&unrelated, 0, &mut plan, None),
                SPIELORDERS_RESULT_OK
            );
            commit(&plan.unwrap());
        }
    }
}

#[test]
fn vendor_deletion_rechecks_new_damaged_references_before_writing() {
    let root = initialized("late-damaged-vendor-reference");
    let package = open(&root.0);
    let mut draft = new_draft(&package, spielorders_package_new_vendor_draft);
    set(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME, "First");
    commit(&plan_draft(&draft).unwrap());
    let package = open(&root.0);
    let first = vendor_named(&package, "First").unwrap();
    let mut deletion = None;
    assert_eq!(
        spielorders_vendor_plan_delete(&first, 0, &mut deletion, None),
        SPIELORDERS_RESULT_OK
    );
    let directory = created_order(&root.0, "Late sample", &[]);
    let package = open(&root.0);
    let mut draft = order_draft(&read_order(&package, &directory));
    set(&mut draft, SPIELORDERS_ORDER_FIELD_VENDOR, "First");
    commit(&plan_draft(&draft).unwrap());
    let record = directory.join(".spielorders/order.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
    document["date_ordered"] = serde_json::json!("2026-02-30");
    fs::write(&record, serde_json::to_vec(&document).unwrap()).unwrap();
    let before = fs::read(root.0.join(".spielorders/vendors.json")).unwrap();
    let mut report = None;
    let mut outcome = None;
    assert_eq!(
        spielorders_plan_commit(&deletion.unwrap(), &mut report, Some(&mut outcome)),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(report.is_none());
    assert!(
        codes(&outcome)
            .iter()
            .any(|code| code == "preconditions_changed")
    );
    assert_eq!(
        fs::read(root.0.join(".spielorders/vendors.json")).unwrap(),
        before
    );
}

#[test]
fn value_matching_agrees_with_adding_an_existing_spelling() {
    for (left, right, expected) in [
        ("Café", "Cafe\u{301}", false),
        ("Café", " CAFÉ ", true),
        ("Straße", "STRASSE", true),
    ] {
        let root = initialized("value-matching");
        let package = open(&root.0);
        let left = c(left);
        let right = c(right);
        let mut plan = None;
        assert_eq!(
            spielorders_package_plan_add_value(
                &package,
                SPIELORDERS_ORDER_FIELD_ORDERER,
                OptionalText::null(),
                Text::new(&left),
                0,
                &mut plan,
                None,
            ),
            SPIELORDERS_RESULT_OK
        );
        commit(&plan.unwrap());
        let mut plan = None;
        assert_eq!(
            spielorders_package_plan_add_value(
                &package,
                SPIELORDERS_ORDER_FIELD_ORDERER,
                OptionalText::null(),
                Text::new(&right),
                0,
                &mut plan,
                None,
            ),
            SPIELORDERS_RESULT_OK
        );
        let report = commit(&plan.unwrap());
        assert_eq!(
            spielorders_report_kind(&report) == SPIELORDERS_REPORT_UNCHANGED,
            expected
        );
        assert_eq!(
            spielorders_value_matches(Text::new(&left), Text::new(&right)) == 1,
            expected
        );
    }
    let invalid = CString::new([0xff]).unwrap();
    assert_eq!(
        spielorders_value_matches(Text::new(&invalid), Text::new(c"Ada")),
        0
    );
}

/// A merge plan says which of the entries it moves or discards are folders.
#[test]
fn a_merge_plan_marks_the_folders_it_moves() {
    let root = initialized("merge-folders");
    let kept = created_order(&root.0, "Kept", &[]);
    let other = created_order(&root.0, "Other", &[]);
    fs::create_dir(other.join("photos")).unwrap();
    fs::write(other.join("photos/bench.jpg"), "photo").unwrap();
    fs::write(other.join("invoice.pdf"), "invoice").unwrap();
    let package = open(&root.0);
    let order = read_order(&package, &kept);
    let merged = c_path(&other);
    let paths = [PathBytes::new(&merged)];
    let mut plan = None;
    assert_eq!(
        spielorders_order_plan_merge(
            &order,
            std::ptr::NonNull::new(paths.as_ptr().cast_mut()),
            1,
            0,
            &mut plan,
            None,
        ),
        SPIELORDERS_RESULT_OK
    );
    let plan = plan.unwrap();
    let moved = (0..spielorders_plan_operation_count(&plan))
        .filter(|index| {
            spielorders_plan_operation_kind(&plan, *index)
                == SPIELORDERS_OPERATION_MOVE_MERGED_ENTRY
        })
        .map(|index| {
            (
                path(spielorders_plan_operation_path(&plan, index))
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                spielorders_plan_operation_directory(&plan, index),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        moved,
        [("invoice.pdf".to_owned(), 0), ("photos".to_owned(), 1)]
    );
}

/// A Package handle opened earlier never stands in for the Vendor record as it is now.
#[test]
fn an_order_read_resolves_its_vendor_as_the_vendor_record_now_names_it() {
    let root = initialized("related-vendor");
    let package = open(&root.0);
    let mut vendor = new_draft(&package, spielorders_package_new_vendor_draft);
    set(&mut vendor, SPIELORDERS_VENDOR_FIELD_NAME, "Acme");
    commit(&plan_draft(&vendor).unwrap());
    let package = open(&root.0);
    let mut order = new_draft(&package, spielorders_package_new_order_draft);
    set(&mut order, SPIELORDERS_ORDER_FIELD_VENDOR, "Acme");
    let report = commit(&plan_draft(&order).unwrap());
    let directory = path(spielorders_report_final_path(&report));
    let package = open(&root.0);
    let vendors = root.0.join(".spielorders/vendors.json");
    let bytes = fs::read_to_string(&vendors).unwrap();
    fs::write(&vendors, bytes.replace("Acme", "Beta")).unwrap();
    let order = read_order(&package, &directory);
    assert_eq!(
        string(spielorders_order_field(
            &order,
            SPIELORDERS_ORDER_FIELD_VENDOR
        )),
        "Beta"
    );
}

#[test]
fn an_order_read_refuses_a_package_damaged_after_it_was_opened() {
    for case in ["config", "version", "duplicate"] {
        let root = initialized(case);
        let directory = created_order(&root.0, "Lenses", &[]);
        let package = open(&root.0);
        match case {
            "config" => fs::write(
                root.0.join(".spielorders/config.toml"),
                "not valid TOML {{{",
            )
            .unwrap(),
            "version" => fs::write(
                root.0.join(".spielorders/version.json"),
                r#"{"metadata_version":99}"#,
            )
            .unwrap(),
            "duplicate" => {
                let duplicate = root.0.join("Orders/Duplicate");
                fs::create_dir_all(duplicate.join(".spielorders")).unwrap();
                fs::copy(
                    directory.join(".spielorders/order.json"),
                    duplicate.join(".spielorders/order.json"),
                )
                .unwrap();
                fs::copy(directory.join("README.md"), duplicate.join("README.md")).unwrap();
            }
            _ => unreachable!(),
        }
        let mut order = None;
        let directory = c_path(&directory);
        assert_eq!(
            spielorders_package_read_order(
                &package,
                PathBytes::new(&directory),
                0,
                &mut order,
                None
            ),
            SPIELORDERS_RESULT_REFUSED,
            "{case}"
        );
        assert!(order.is_none());
    }
}

/// A Vendor that only the derived index names is no Vendor, and a save naming it is
/// refused at the Order's Vendor field.
#[test]
fn an_order_save_names_only_vendors_the_vendor_record_holds() {
    let root = initialized("index-vendor");
    let directory = created_order(&root.0, "Lenses", &[]);
    let index = root.0.join(".spielorders/index.json");
    let original = fs::read_to_string(&index).unwrap();
    let invented = original.replace("\"vendors\": []", "\"vendors\": [\n    {\n      \"vendor_id\": \"11111111-1111-4111-8111-111111111111\",\n      \"name\": \"Ghost\",\n      \"contacts\": []\n    }\n  ]");
    assert_ne!(invented, original);
    fs::write(&index, invented).unwrap();
    let package = open(&root.0);
    let order = read_order(&package, &directory);
    let mut draft = order_draft(&order);
    set(&mut draft, SPIELORDERS_ORDER_FIELD_VENDOR, "Ghost");
    let before = tree(&root.0);
    assert_eq!(
        plan_draft(&draft).map(|_| ()),
        Err(vec![("unknown_vendor".into(), "vendor".into(), -1)])
    );
    assert_eq!(tree(&root.0), before);
}

#[test]
fn a_linked_order_metadata_directory_lists_as_damaged_and_is_never_written() {
    let root = initialized("linked-browsing");
    let order = created_order(&root.0, "Lenses", &[]);
    let outside = Directory::new("linked-record");
    let metadata = order.join(".spielorders");
    fs::rename(&metadata, outside.0.join("record")).unwrap();
    std::os::unix::fs::symlink(outside.0.join("record"), &metadata).unwrap();
    let before = fs::read(outside.0.join("record/order.json")).unwrap();
    let package = open(&root.0);
    let list = orders(&package, None, 0, SpielordersConstant(-1), 0);
    assert_eq!(spielorders_list_count(&list), 1);
    assert!(
        !string(spielorders_list_field(
            &list,
            0,
            SPIELORDERS_ORDER_FIELD_DAMAGE
        ))
        .is_empty()
    );
    assert_eq!(
        spielorders_package_index_state(&package),
        SPIELORDERS_INDEX_STATE_STALE
    );
    assert_eq!(
        fs::read(outside.0.join("record/order.json")).unwrap(),
        before
    );
}

fn title_batch(package: &SpielordersPackage, directory: &Path) -> Box<SpielordersPlan> {
    let mut batch = None;
    assert_eq!(
        spielorders_package_batch(package, &mut batch, None),
        SPIELORDERS_RESULT_OK
    );
    let mut batch = batch.unwrap();
    let directory = c_path(directory);
    let mut draft = None;
    assert_eq!(
        spielorders_batch_order_draft(&batch, PathBytes::new(&directory), &mut draft, None,),
        SPIELORDERS_RESULT_OK
    );
    let mut draft = draft.unwrap();
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Changed title");
    assert_eq!(
        spielorders_batch_add(&mut batch, &draft, 0, None),
        SPIELORDERS_RESULT_OK
    );
    let mut plan = None;
    assert_eq!(
        spielorders_batch_plan(&batch, 0, &mut plan, None),
        SPIELORDERS_RESULT_OK
    );
    plan.unwrap()
}

#[test]
fn a_batch_refuses_orders_that_leave_its_active_scope() {
    let mut observed = Vec::new();
    for case in [
        "configured elsewhere",
        "linked directory",
        "linked metadata",
        "linked record",
        "removed",
    ] {
        let root = initialized("batch-scope");
        let directory = created_order(&root.0, "Lenses", &[]);
        let package = open(&root.0);
        let plan = title_batch(&package, &directory);
        let outside = Directory::new("batch-scope-outside");
        let metadata = directory.join(".spielorders");
        let record = metadata.join("order.json");
        match case {
            "configured elsewhere" => {
                fs::create_dir(root.0.join("NewOrders")).unwrap();
                fs::write(
                    root.0.join(".spielorders/config.toml"),
                    "orders_path = 'NewOrders'\n",
                )
                .unwrap();
            }
            "linked directory" => {
                let moved = outside.0.join("Order");
                fs::rename(&directory, &moved).unwrap();
                std::os::unix::fs::symlink(moved, &directory).unwrap();
            }
            "linked metadata" => {
                let moved = outside.0.join("metadata");
                fs::rename(&metadata, &moved).unwrap();
                std::os::unix::fs::symlink(moved, &metadata).unwrap();
            }
            "linked record" => {
                let moved = outside.0.join("order.json");
                fs::rename(&record, &moved).unwrap();
                std::os::unix::fs::symlink(moved, &record).unwrap();
            }
            "removed" => fs::rename(&directory, outside.0.join("Order")).unwrap(),
            _ => unreachable!(),
        }
        let before = tree(&root.0);
        let outside_before = tree(&outside.0);
        let mut report = None;
        let result = spielorders_plan_commit(&plan, &mut report, None);
        observed.push((
            case,
            result,
            report.is_none(),
            tree(&root.0) == before,
            tree(&outside.0) == outside_before,
        ));
    }
    assert!(
        observed
            .iter()
            .all(|(_, result, no_report, same_root, same_outside)| {
                *result == SPIELORDERS_RESULT_REFUSED && *no_report && *same_root && *same_outside
            }),
        "a stale scope must authorize no write: {observed:?}"
    );
}

#[test]
fn a_batch_add_refuses_linked_order_records() {
    let mut observed = Vec::new();
    for case in ["directory", "metadata", "record"] {
        let root = initialized("batch-add-kind");
        let directory = created_order(&root.0, "Lenses", &[]);
        let package = open(&root.0);
        let mut batch = None;
        assert_eq!(
            spielorders_package_batch(&package, &mut batch, None),
            SPIELORDERS_RESULT_OK
        );
        let mut batch = batch.unwrap();
        let argument = c_path(&directory);
        let mut draft = None;
        assert_eq!(
            spielorders_batch_order_draft(&batch, PathBytes::new(&argument), &mut draft, None),
            SPIELORDERS_RESULT_OK
        );
        let mut draft = draft.unwrap();
        set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Changed title");
        let outside = Directory::new("batch-add-kind-outside");
        let source = match case {
            "directory" => directory,
            "metadata" => directory.join(".spielorders"),
            "record" => directory.join(".spielorders/order.json"),
            _ => unreachable!(),
        };
        let moved = outside.0.join("source");
        fs::rename(&source, &moved).unwrap();
        std::os::unix::fs::symlink(moved, &source).unwrap();
        let before = tree(&root.0);
        let outside_before = tree(&outside.0);
        observed.push((
            case,
            spielorders_batch_add(&mut batch, &draft, 0, None),
            spielorders_batch_count(&batch),
            tree(&root.0) == before,
            tree(&outside.0) == outside_before,
        ));
    }
    assert!(
        observed
            .iter()
            .all(|(_, result, count, same_root, same_outside)| {
                *result == SPIELORDERS_RESULT_REFUSED && *count == 0 && *same_root && *same_outside
            }),
        "a linked source must not enter the batch: {observed:?}"
    );
}

#[test]
fn an_edit_refuses_a_linked_record_after_planning() {
    let root = initialized("edit-linked-record");
    let directory = created_order(&root.0, "Lenses", &[]);
    let package = open(&root.0);
    let order = read_order(&package, &directory);
    let mut draft = order_draft(&order);
    set(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE, "Changed title");
    let plan = plan_draft(&draft).unwrap();
    let outside = Directory::new("edit-linked-record-outside");
    let record = directory.join(".spielorders/order.json");
    let moved = outside.0.join("order.json");
    fs::rename(&record, &moved).unwrap();
    std::os::unix::fs::symlink(moved, &record).unwrap();
    let before = tree(&root.0);
    let outside_before = tree(&outside.0);
    let mut report = None;
    assert_eq!(
        spielorders_plan_commit(&plan, &mut report, None),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(report.is_none());
    assert_eq!(tree(&root.0), before);
    assert_eq!(tree(&outside.0), outside_before);
}

#[test]
fn a_merge_refuses_a_later_record_linked_after_planning() {
    let root = initialized("merge-linked-record");
    let kept = created_order(&root.0, "Kept", &[]);
    let other = created_order(&root.0, "Other", &[("Lens", "1", "2")]);
    let package = open(&root.0);
    let kept = read_order(&package, &kept);
    let argument = c_path(&other);
    let paths = [PathBytes::new(&argument)];
    let mut plan = None;
    assert_eq!(
        spielorders_order_plan_merge(
            &kept,
            std::ptr::NonNull::new(paths.as_ptr().cast_mut()),
            1,
            0,
            &mut plan,
            None,
        ),
        SPIELORDERS_RESULT_OK
    );
    let outside = Directory::new("merge-linked-record-outside");
    let record = other.join(".spielorders/order.json");
    let moved = outside.0.join("order.json");
    fs::rename(&record, &moved).unwrap();
    std::os::unix::fs::symlink(moved, &record).unwrap();
    let before = tree(&root.0);
    let outside_before = tree(&outside.0);
    let mut report = None;
    assert_eq!(
        spielorders_plan_commit(&plan.unwrap(), &mut report, None),
        SPIELORDERS_RESULT_REFUSED
    );
    assert!(report.is_none());
    assert_eq!(tree(&root.0), before);
    assert_eq!(tree(&outside.0), outside_before);
}

#[test]
fn a_package_refuses_an_order_directory_of_another_package() {
    for nested in [false, true] {
        let root = initialized("another-package");
        let elsewhere = initialized("another-package-elsewhere");
        let directory = created_order(&elsewhere.0, "Mounts", &[]);
        let directory = if nested {
            let nested_root = root.0.join("Other Package");
            fs::rename(&elsewhere.0, &nested_root).unwrap();
            nested_root.join(directory.strip_prefix(&elsewhere.0).unwrap())
        } else {
            directory
        };
        let theirs = c_path(&directory);
        let package = open(&root.0);

        let mut order = None;
        let mut outcome = None;
        let result = spielorders_package_read_order(
            &package,
            PathBytes::new(&theirs),
            0,
            &mut order,
            Some(&mut outcome),
        );
        assert_eq!(result, SPIELORDERS_RESULT_REFUSED);
        assert!(order.is_none());
        assert_eq!(codes(&outcome), ["order_not_found"]);

        for validating in [true, false] {
            let mut diagnosis = None;
            let mut outcome = None;
            let directory = PathBytes::new(&theirs);
            let result = if validating {
                spielorders_package_validate_order(
                    &package,
                    directory,
                    0,
                    &mut diagnosis,
                    Some(&mut outcome),
                )
            } else {
                spielorders_package_diagnose_order(
                    &package,
                    directory,
                    0,
                    &mut diagnosis,
                    Some(&mut outcome),
                )
            };
            assert_eq!(result, SPIELORDERS_RESULT_REFUSED);
            assert!(diagnosis.is_none());
            assert_eq!(codes(&outcome), ["order_not_found"]);
        }
    }
}

#[test]
fn an_assignment_is_refused_on_its_own_terms_by_its_record() {
    let refusal = |family, name: &str, value: &str| {
        let (name, value) = (c(name), c(value));
        string(spielorders_constant_name(spielorders_assignment_refusal(
            family,
            Text::new(&name),
            Text::new(&value),
        )))
    };
    let cases = [
        (SPIELORDERS_FAMILY_ORDER_FIELD, "title", "Lenses"),
        (SPIELORDERS_FAMILY_ORDER_FIELD, "title", " "),
        (SPIELORDERS_FAMILY_ORDER_FIELD, "date_ordered", ""),
        (SPIELORDERS_FAMILY_ORDER_FIELD, "order_id", "x"),
        (SPIELORDERS_FAMILY_ORDER_FIELD, "package_root", "x"),
        (SPIELORDERS_FAMILY_ORDER_FIELD, "items", "x"),
        (SPIELORDERS_FAMILY_ORDER_FIELD, "total", "1"),
        (SPIELORDERS_FAMILY_ITEM_FIELD, "quantity", "two"),
        (SPIELORDERS_FAMILY_ITEM_FIELD, "catalog_number", ""),
        (SPIELORDERS_FAMILY_VENDOR_FIELD, "vendor_id", "x"),
        (SPIELORDERS_FAMILY_VENDOR_FIELD, "contacts", "x"),
        (SPIELORDERS_FAMILY_CONTACT_FIELD, "name", ""),
        (SPIELORDERS_FAMILY_TEXT_FIELD, "content", "x"),
    ];
    assert_eq!(
        cases.map(|(family, name, value)| refusal(family, name, value)),
        [
            "",
            "blank_required_value",
            "blank_required_value",
            "immutable_identity",
            "managed_technical_field",
            "ordered_collection",
            "unknown_field",
            "not_a_finite_number",
            "",
            "immutable_identity",
            "ordered_collection",
            "blank_required_value",
            "unknown_field",
        ]
    );
}

#[test]
fn the_orders_using_a_value_are_those_its_count_includes() {
    let root = initialized("orders-with-value");
    let facts = [
        ("Chamber", "Ada", "Clock", "Lab"),
        ("Oven", " ada ", "clock", "lab"),
        ("Lamp", "Grace", "", "Lab"),
    ];
    for (title, orderer, project, group) in facts {
        let directory = created_order(&root.0, title, &[]);
        let mut draft = order_draft(&read_order(&open(&root.0), &directory));
        for (field, value) in [
            (SPIELORDERS_ORDER_FIELD_ORDERER, orderer),
            (SPIELORDERS_ORDER_FIELD_PROJECT, project),
            (SPIELORDERS_ORDER_FIELD_ORDER_GROUP, group),
        ] {
            set(&mut draft, field, value);
            spielorders_draft_confirm(&mut draft, field, -1, None);
        }
        commit(&plan_draft(&draft).unwrap());
    }
    let package = open(&root.0);
    let using = |field, value: &str, project: Option<&str>| {
        let value = c(value);
        let project = project.map(c);
        let mut list = None;
        let result = spielorders_package_orders_with_value(
            &package,
            field,
            Text::new(&value),
            project
                .as_deref()
                .map_or(OptionalText::null(), OptionalText::new),
            0,
            &mut list,
            None,
        );
        (result == SPIELORDERS_RESULT_OK).then(|| titles(&list.unwrap()))
    };

    assert_eq!(
        [
            using(SPIELORDERS_ORDER_FIELD_ORDERER, "ADA", None),
            using(SPIELORDERS_ORDER_FIELD_PROJECT, "Clock", None),
            using(SPIELORDERS_ORDER_FIELD_ORDER_GROUP, "LAB", Some("clock")),
            using(SPIELORDERS_ORDER_FIELD_ORDER_GROUP, "Lab", None),
            using(SPIELORDERS_ORDER_FIELD_TITLE, "Oven", None),
        ],
        [
            Some(vec!["Chamber".to_owned(), "Oven".to_owned()]),
            Some(vec!["Chamber".to_owned(), "Oven".to_owned()]),
            Some(vec!["Chamber".to_owned(), "Oven".to_owned()]),
            Some(vec!["Lamp".to_owned()]),
            None,
        ]
    );
    let mut values = None;
    spielorders_package_values(&package, SPIELORDERS_ORDER_FIELD_ORDERER, &mut values, None);
    let values = values.unwrap();
    let ada = (0..spielorders_list_count(&values))
        .find(|row| {
            string(spielorders_list_field(
                &values,
                *row,
                SPIELORDERS_VALUE_FIELD_VALUE,
            )) == "Ada"
        })
        .unwrap();
    assert_eq!(
        string(spielorders_list_field(
            &values,
            ada,
            SPIELORDERS_VALUE_FIELD_USAGE_COUNT
        )),
        "2"
    );
}

#[test]
fn an_unreadable_readme_is_reported_once_by_an_order_read() {
    let root = initialized("readme-reported-once");
    let directory = created_order(&root.0, "Lenses", &[]);
    fs::remove_file(directory.join("README.md")).unwrap();
    fs::create_dir(directory.join("README.md")).unwrap();
    let package = open(&root.0);
    let directory = c_path(&directory);
    let mut order = None;
    let mut outcome = None;
    spielorders_package_read_order(
        &package,
        PathBytes::new(&directory),
        0,
        &mut order,
        Some(&mut outcome),
    );
    assert_eq!(codes(&outcome), ["readme_unavailable"]);
}

#[test]
fn a_fault_inside_an_item_names_that_items_field_and_row() {
    let root = initialized("nested-field-fault");
    let directory = created_order(&root.0, "Lenses", &[("Lens", "1", "2")]);
    let record = directory.join(".spielorders/order.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
    document["items"][0]["quantity"] = serde_json::json!("two");
    fs::write(&record, serde_json::to_vec(&document).unwrap()).unwrap();
    let package = open(&root.0);
    let directory = c_path(&directory);
    let mut order = None;
    let mut outcome = None;
    spielorders_package_read_order(
        &package,
        PathBytes::new(&directory),
        0,
        &mut order,
        Some(&mut outcome),
    );
    assert!(
        located(&outcome).contains(&("damaged_record".to_owned(), "quantity".to_owned(), 0)),
        "{:?}",
        located(&outcome)
    );
}
