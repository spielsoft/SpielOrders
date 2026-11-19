use crate::ports::{
    DirectoryEntry, FileChangeEvidence, FileSystem, FileSystemEntryKind, LocalDate,
};
use crate::{
    Assignments, Change, ChangeResult, Diagnosis, Failure, OrderRef, Package, Plan, Report, Scope,
    VendorRef, commit, plan, readme_writes::ReadmeOriginal, repair_cases::RepairAction,
};
use crate::{change::PlannedAct, mutation::CleanupFailure, mutation::CommitFailure};
use crate::{
    metadata::OrderDocument, metadata::Vendor, operations::FailedPath, readme::Diagnostic,
    readme::DiagnosticCode, readme::DiagnosticEvidence, readme::Location, readme::Severity,
    repair_cases::RepairCase,
};
use std::ops::Deref;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

mod scripted;
mod syncs;

pub use scripted::{Answer, Operation, Outside, Script, ScriptedFileSystem};
pub use syncs::{Sync, Syncs};

pub use crate::platform::PlatformFileSystem;

/// Whether the volume holding `directory` itself resolves two spellings that differ
/// only in case to one entry.
///
/// Measured rather than assumed: case sensitivity belongs to the volume the test
/// writes on, not to the platform it runs on.
pub fn resolves_case_insensitively(directory: &Path) -> bool {
    let probe = directory.join("CaseEquivalenceProbe");
    std::fs::create_dir(&probe).expect("the case probe directory is created");
    let merged = std::fs::symlink_metadata(directory.join("caseequivalenceprobe")).is_ok();
    std::fs::remove_dir(&probe).expect("the case probe directory is removed");
    merged
}

pub struct TemporaryTree(PathBuf);

/// Distinguishes trees claimed within one process. The whole suite is one test
/// binary, so two tests that pass the same label now share a process id, and a
/// clock read is not guaranteed to separate two threads that reach it together.
static TREE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl TemporaryTree {
    pub fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock follows the Unix epoch")
            .as_nanos();
        let sequence = TREE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "spielorders-{label}-{}-{nonce}-{sequence}",
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
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("temporary test tree is removed");
    }
}

/// The one variant `value` is expected to be, unwrapped.
macro_rules! variant {
    ($value:expr, $variant:path) => {
        match $value {
            $variant(inner) => inner,
            other => panic!("expected {}: {other:?}", stringify!($variant)),
        }
    };
}

/// A calendar on which every day is the same day.
pub struct FixedCalendar(LocalDate);

impl FixedCalendar {
    pub fn on(year: i32, month: u8, day: u8) -> Self {
        Self(LocalDate { year, month, day })
    }
}

/// Where a test mutation's new identities come from.
pub trait UuidGenerator {
    fn generate_v4(&mut self) -> String;
}

impl<T: UuidGenerator + ?Sized> UuidGenerator for &mut T {
    fn generate_v4(&mut self) -> String {
        (**self).generate_v4()
    }
}

/// A UUID source that hands out one prepared identity every time it is asked.
pub struct FixedUuid(pub String);

impl FixedUuid {
    pub fn new(value: &str) -> Self {
        Self(value.into())
    }

    /// The identity it hands out.
    pub fn value(&self) -> &str {
        &self.0
    }
}

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.clone()
    }
}

/// A UUID source that hands out prepared identities in order, so a fixture's Order and
/// Vendor identifiers are the ones the test names.
pub struct QueuedUuids(Vec<String>);

impl QueuedUuids {
    pub fn of(values: &[&str]) -> Self {
        Self(values.iter().rev().map(|value| value.to_string()).collect())
    }
}

impl UuidGenerator for QueuedUuids {
    fn generate_v4(&mut self) -> String {
        self.0.pop().expect("a prepared identity")
    }
}

struct NoIdentity;

impl UuidGenerator for NoIdentity {
    fn generate_v4(&mut self) -> String {
        panic!("this mutation mints no identity")
    }
}

/// Every effect a test mutation draws on: one filesystem, a fixed calendar day, and a
/// UUID source. A test that supplies no UUID source is saying the mutation must not reach
/// for one.
pub struct TestEffects<'a, F: FileSystem> {
    filesystem: &'a F,
    today: LocalDate,
    identities: RefCell<Box<dyn UuidGenerator + 'a>>,
}

/// [`TestEffects`] over `filesystem`, on 22 August 2026, minting nothing.
pub fn effects<F: FileSystem>(filesystem: &F) -> TestEffects<'_, F> {
    TestEffects {
        filesystem,
        today: LocalDate {
            year: 2026,
            month: 8,
            day: 22,
        },
        identities: RefCell::new(Box::new(NoIdentity)),
    }
}

impl<'a, F: FileSystem> TestEffects<'a, F> {
    /// The same effects on `calendar`'s day.
    pub fn dated(mut self, calendar: &FixedCalendar) -> Self {
        self.today = calendar.0;
        self
    }

    /// The same effects on another day.
    pub fn on(self, year: i32, month: u8, day: u8) -> Self {
        self.dated(&FixedCalendar::on(year, month, day))
    }

    /// The same effects, minting identities from `identities`.
    pub fn minting(self, identities: impl UuidGenerator + 'a) -> Self {
        *self.identities.borrow_mut() = Box::new(identities);
        self
    }
}

impl<F: FileSystem> crate::ports::Effects for TestEffects<'_, F> {
    type FileSystem = F;

    fn file_system(&self) -> &F {
        self.filesystem
    }

    fn today(&self) -> LocalDate {
        self.today
    }

    fn new_uuid(&self) -> String {
        self.identities.borrow_mut().generate_v4()
    }
}

/// A caller that never cancels.
pub static UNCANCELLED: AtomicBool = AtomicBool::new(false);

/// The Package at `root`, opened over `filesystem`.
pub fn opened<F: FileSystem>(root: &Path, filesystem: &F) -> crate::Answer<Package> {
    crate::open(root, &effects(filesystem), &UNCANCELLED)
}

/// The whole Package at `root`, validated over `filesystem`.
pub fn validated_package<F: FileSystem>(root: &Path, filesystem: &F) -> crate::Answer<Diagnosis> {
    crate::validate(
        &Scope::Package(root.into()),
        &effects(filesystem),
        &UNCANCELLED,
    )
}

/// The Order Directory `directory`, validated over `filesystem`.
pub fn validated_order<F: FileSystem>(
    directory: &Path,
    filesystem: &F,
) -> crate::Answer<Diagnosis> {
    crate::validate(
        &Scope::Order(OrderRef::Directory(directory.into())),
        &effects(filesystem),
        &UNCANCELLED,
    )
}

/// The repairs the Package at `root` offers for its own artifacts.
pub fn diagnosed_package<F: FileSystem>(root: &Path, filesystem: &F) -> crate::Answer<Diagnosis> {
    crate::diagnose(
        &Scope::Package(root.into()),
        &effects(filesystem),
        &UNCANCELLED,
    )
}

/// The repairs the Order Directory `directory` offers.
pub fn diagnosed_order<F: FileSystem>(
    directory: &Path,
    filesystem: &F,
) -> crate::Answer<Diagnosis> {
    crate::diagnose(
        &Scope::Order(OrderRef::Directory(directory.into())),
        &effects(filesystem),
        &UNCANCELLED,
    )
}

/// The Order whose Directory is `directory`, read over `filesystem`.
pub fn order_at<F: FileSystem>(
    directory: &Path,
    filesystem: &F,
) -> crate::Answer<crate::OrderRead> {
    crate::read_order(&OrderRef::Directory(directory.into()), &effects(filesystem))
}

/// The Vendor named `name` in the Package at `root`, read over `filesystem`.
pub fn vendor_named<F: FileSystem>(
    root: &Path,
    name: &str,
    filesystem: &F,
) -> crate::Answer<crate::VendorRead> {
    crate::read_vendor(root, &VendorRef::Name(name.into()), &effects(filesystem))
}

/// The adoption of `directory` into the Package at `root`, with the Order's facts and
/// each Item's.
pub fn adoption(
    root: impl Into<PathBuf>,
    directory: impl Into<PathBuf>,
    (assignments, items): &(Assignments, Vec<Assignments>),
) -> Change {
    Change::AdoptOrder {
        root: root.into(),
        directory: directory.into(),
        assignments: assignments.clone(),
        items: items.clone(),
    }
}

/// The repair of the case `case` in `scope`, by `action` with `values`.
pub fn repair(
    scope: Scope,
    case: impl Into<String>,
    action: RepairAction,
    values: Assignments,
) -> Change {
    Change::Repair {
        scope,
        case: case.into(),
        action,
        values,
    }
}

/// The Package at `root`, as a scope.
pub fn package_scope(root: impl Into<PathBuf>) -> Scope {
    Scope::Package(root.into())
}

/// The Order whose Directory is `directory`, as a scope.
pub fn order_scope(directory: impl Into<PathBuf>) -> Scope {
    Scope::Order(OrderRef::Directory(directory.into()))
}

/// The Order whose Directory is `directory`.
pub fn order(directory: impl Into<PathBuf>) -> OrderRef {
    OrderRef::Directory(directory.into())
}

/// A new Order in the Package at `root`.
pub fn create_order(root: impl Into<PathBuf>, assignments: Assignments) -> Change {
    Change::CreateOrder {
        root: root.into(),
        assignments,
        items: Vec::new(),
    }
}

/// The Order record stored at `directory`, read straight from the disk the suite built.
fn stored_order(directory: &Path) -> Option<OrderDocument> {
    crate::read_order(&order(directory), &effects(&PlatformFileSystem))
        .result
        .ok()
        .map(|read| read.order.document)
}

/// One stored Item, as the row an editor's draft holds for it.
fn item_row(item: &crate::metadata::OrderItem) -> Assignments {
    let row = Assignments::new()
        .assign("description", item.description.to_string())
        .assign("quantity", item.quantity.to_string())
        .assign("unit_price", item.unit_price.to_string());
    match &item.catalog_number {
        Some(catalog_number) => row.assign("catalog_number", catalog_number.clone()),
        None => row,
    }
}

/// An editor's save of the Order whose Directory is `directory`, drafted from the record
/// stored there: `assignments` over its facts, its Items as stored plus `added`.
fn saved_order(directory: PathBuf, assignments: Assignments, added: Option<Assignments>) -> Change {
    let stored = stored_order(&directory);
    let mut items: Vec<Assignments> = stored
        .iter()
        .flat_map(|document| document.items.iter().map(item_row))
        .collect();
    items.extend(added);
    let original = stored.unwrap_or_else(|| {
        OrderDocument::from_json_bytes(include_bytes!(
            "../../../../../contracts/examples/v1/minimal/order.json"
        ))
        .expect("the minimal example is an Order record")
    });
    Change::SaveOrder {
        order: order(directory),
        assignments,
        items,
        original,
    }
}

/// An edit of the facts of the Order whose Directory is `directory`.
pub fn edit_order(directory: impl Into<PathBuf>, assignments: Assignments) -> Change {
    saved_order(directory.into(), assignments, None)
}

/// One Item appended to the Order whose Directory is `directory`.
pub fn add_item(directory: impl Into<PathBuf>, item: Assignments) -> Change {
    saved_order(directory.into(), Assignments::new(), Some(item))
}

/// The alignment of the Order whose Directory is `directory`.
pub fn align_order(directory: impl Into<PathBuf>) -> Change {
    Change::AlignOrder {
        order: order(directory),
    }
}

/// The removal of the Order whose Directory is `directory`.
pub fn remove_order(directory: impl Into<PathBuf>) -> Change {
    Change::RemoveOrder {
        order: order(directory),
    }
}

/// A new Vendor in the Package at `root`.
pub fn create_vendor(root: impl Into<PathBuf>, assignments: Assignments) -> Change {
    Change::CreateVendor {
        root: root.into(),
        assignments,
        contacts: Vec::new(),
    }
}

/// The Vendor `vendor` names in the aggregate stored in the Package at `root`.
fn stored_vendor(root: &Path, vendor: &VendorRef) -> Option<Vendor> {
    crate::read_vendor(root, vendor, &effects(&PlatformFileSystem))
        .result
        .ok()
        .map(|read| read.vendor)
}

/// An editor's save of `vendor`, drafted from the aggregate stored at `root`:
/// `assignments` over its facts, its Contacts as stored plus `added`.
fn saved_vendor(
    root: PathBuf,
    vendor: VendorRef,
    assignments: Assignments,
    added: Option<Assignments>,
) -> Change {
    let stored = stored_vendor(&root, &vendor);
    let mut contacts: Vec<Assignments> = stored
        .iter()
        .flat_map(|vendor| {
            vendor.contacts.iter().map(|contact| {
                [
                    ("role", &contact.role),
                    ("phone", &contact.phone),
                    ("email", &contact.email),
                ]
                .into_iter()
                .fold(
                    Assignments::new().assign("name", contact.name.to_string()),
                    |row, (field, value)| match value {
                        Some(value) => row.assign(field, value.clone()),
                        None => row,
                    },
                )
            })
        })
        .collect();
    contacts.extend(added);
    let original = stored.unwrap_or_else(|| {
        serde_json::from_value(serde_json::json!({
            "vendor_id": match &vendor {
                VendorRef::Id(id)
                    if serde_json::from_value::<crate::metadata::VendorId>(
                        serde_json::json!(id),
                    )
                    .is_ok() =>
                {
                    id.clone()
                }
                _ => "6d1c2a4e-0b5f-4c3a-9e7d-2f8a1b3c4d5e".to_owned(),
            },
            "name": "Absent",
            "contacts": [],
        }))
        .expect("a placeholder Vendor")
    });
    Change::SaveVendor {
        root,
        assignments,
        contacts,
        original,
    }
}

/// An edit of `vendor`'s facts.
pub fn edit_vendor(
    root: impl Into<PathBuf>,
    vendor: VendorRef,
    assignments: Assignments,
) -> Change {
    saved_vendor(root.into(), vendor, assignments, None)
}

/// One Contact appended to `vendor`.
pub fn add_contact(root: impl Into<PathBuf>, vendor: VendorRef, contact: Assignments) -> Change {
    saved_vendor(root.into(), vendor, Assignments::new(), Some(contact))
}

/// Hiding, or unhiding, `vendor`.
pub fn set_vendor_hidden(root: impl Into<PathBuf>, vendor: VendorRef, hidden: bool) -> Change {
    Change::SetVendorHidden {
        root: root.into(),
        vendor,
        hidden,
    }
}

/// An editor's Save of the Order whose Directory is `directory`: its facts and its
/// complete Item list, drafted from `original`.
pub fn save_order(
    directory: impl Into<PathBuf>,
    original: OrderDocument,
    assignments: Assignments,
    items: Vec<Assignments>,
) -> Change {
    Change::SaveOrder {
        order: order(directory),
        assignments,
        items,
        original,
    }
}

/// An editor's Save of the Vendor `original` in the Package at `root`: its facts and its
/// complete Contact list.
pub fn save_vendor(
    root: impl Into<PathBuf>,
    original: Vendor,
    assignments: Assignments,
    contacts: Vec<Assignments>,
) -> Change {
    Change::SaveVendor {
        root: root.into(),
        assignments,
        contacts,
        original,
    }
}

/// Every file under `root` and its bytes, by path; a directory has none.
pub fn snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = std::collections::BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                entries.insert(path.clone(), None);
                pending.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                entries.insert(path, Some(bytes));
            }
        }
    }
    entries
}

/// The initialization of a Package at `root`, at its conventional layout.
pub fn initialize(root: impl Into<PathBuf>) -> Change {
    Change::InitializePackage {
        root: root.into(),
        orders_path: None,
        removed_path: None,
    }
}

/// The rebuild of the derived Package Index of the Package at `root`.
pub fn rebuild_index(root: impl Into<PathBuf>) -> Change {
    Change::RebuildIndex { root: root.into() }
}

/// A write of `text` to the README of `scope`, replacing the document `original` read.
pub fn write_readme(scope: Scope, original: ReadmeOriginal, text: impl Into<String>) -> Change {
    Change::WriteReadme {
        scope,
        original,
        text: text.into(),
    }
}

/// A derived Package Index as `contracts/schemas/v1/index.schema.json` defines it, which is
/// how a test reads one the core wrote or writes one for the core to read.
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct Index {
    pub schema_version: u8,
    pub orders: Vec<IndexedOrder>,
    pub vendors: Vec<Vendor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<crate::metadata::StoredValues>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct IndexedOrder {
    pub order_directory: String,
    pub order: OrderDocument,
    pub readme: String,
}

/// A Vendor aggregate as `contracts/schemas/v1/vendors.schema.json` defines it.
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct Vendors {
    pub schema_version: u8,
    pub vendors: Vec<Vendor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<crate::metadata::StoredValues>,
}

/// A managed JSON document's one deterministic spelling: pretty-printed in field order,
/// ending in a newline.
pub fn canonical_bytes(document: &impl serde::Serialize) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(document).expect("a document serializes");
    bytes.push(b'\n');
    bytes
}

macro_rules! document {
    ($document:ty) => {
        impl $document {
            pub fn from_json_bytes(bytes: &[u8]) -> serde_json::Result<Self> {
                serde_json::from_slice(bytes)
            }

            pub fn to_json_bytes(&self) -> serde_json::Result<Vec<u8>> {
                Ok(canonical_bytes(self))
            }
        }
    };
}

document!(Index);
document!(Vendors);

/// What `value` spells as JSON, which is how a test reads a record the core writes.
pub fn json(value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).expect("a core value serializes")
}

/// The text a string-valued core value, such as an identity or a non-blank name,
/// serializes as.
pub fn text(value: &impl serde::Serialize) -> String {
    match json(value) {
        serde_json::Value::String(text) => text,
        other => panic!("{other} is not text"),
    }
}

/// How [`planned_reuse`] spells one Reusable Free-Entry Value: `reused` or `new`, with
/// its spelling.
pub fn reusable(kind: &str, value: &str) -> serde_json::Value {
    serde_json::json!({ "kind": kind, "value": value })
}

/// The name a planned act is published under: its variant's name in snake case, as its
/// `SPIELORDERS_OPERATION_*` constant spells it.
fn act_name(act: PlannedAct) -> String {
    let variant = match act {
        PlannedAct::Managed(kind) => format!("{kind:?}"),
        PlannedAct::Creation(act) => format!("{act:?}"),
    };
    let mut spelling = String::new();
    for character in variant.chars() {
        if character.is_ascii_uppercase() && !spelling.is_empty() {
            spelling.push('_');
        }
        spelling.push(character.to_ascii_lowercase());
    }
    spelling
}

/// Each operation committing `plan` performs, as its kind and the path it acts on, in
/// order, which is what a dry run shows a caller.
pub fn operations(plan: &Plan) -> Vec<String> {
    plan.operations()
        .into_iter()
        .map(|(act, at)| {
            format!(
                "{} {}",
                act_name(act),
                at.package_relative_path
                    .as_deref()
                    .expect("a planned operation is inside the Package")
            )
        })
        .collect()
}

/// The kinds of the operations `plan` would perform, in order.
pub fn operation_kinds(plan: &Plan) -> Vec<String> {
    plan.operations()
        .into_iter()
        .map(|(act, _)| act_name(act))
        .collect()
}

/// The path the first operation of `kind` in `plan` acts on.
pub fn operation_path(plan: &Plan, kind: &str) -> PathBuf {
    plan.operations()
        .into_iter()
        .find(|(each, _)| act_name(*each) == kind)
        .map(|(_, at)| at.absolute_path.clone())
        .unwrap_or_else(|| panic!("the plan performs {kind}: {:?}", plan.operations()))
}

/// The Order a committed change left, read back from its Directory the way any caller
/// reads it.
pub fn committed_order(result: &ChangeResult) -> crate::canonical::HealthyOrder {
    let directory = match result {
        ChangeResult::CreateOrder(result) => &result.directory,
        ChangeResult::AdoptOrder(result) => &result.directory,
        ChangeResult::EditOrder(result) => &result.directory,
        ChangeResult::AlignOrder(result) => &result.directory,
        other => panic!("{other:?} leaves no Order"),
    };
    order_at(&directory.absolute_path, &PlatformFileSystem)
        .result
        .unwrap_or_else(|failure| panic!("the committed Order reads: {failure:?}"))
        .order
}

/// The Vendor a committed change saved in the Package at `root`, read back by its
/// identity the way any caller reads it.
pub fn committed_vendor(root: &Path, result: &ChangeResult) -> Vendor {
    let ChangeResult::Vendor(result) = result else {
        panic!("{result:?} saves no Vendor");
    };
    crate::read_vendor(
        root,
        &VendorRef::Id(result.vendor_id.to_string()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap_or_else(|failure| panic!("the committed Vendor reads: {failure:?}"))
    .vendor
}

/// Whether the Package at `root` holds a derived index its open proves current, which the
/// open then leaves exactly as it found it.
pub fn index_is_current(root: &Path) -> bool {
    let filesystem = ScriptedFileSystem::new();
    let package = opened(root, &filesystem)
        .result
        .unwrap_or_else(|failure| panic!("the Package opens: {failure:?}"));
    package.contents.index.state == crate::canonical::PackageIndexState::Current
        && filesystem.effects().is_empty()
}

/// The Order Directory an adoption `plan` makes canonical: the one whose metadata
/// directory it creates.
pub fn adopted_directory(plan: &Plan) -> PathBuf {
    operation_path(plan, "create_metadata_directory")
        .parent()
        .expect("a metadata directory is inside its Order Directory")
        .to_path_buf()
}

/// The Order record `plan` would write.
pub fn planned_order(plan: &Plan) -> &OrderDocument {
    &plan
        .order_record()
        .expect("the plan writes an Order record")
        .document
}

/// The Vendor the Order record `plan` would write references.
pub fn planned_order_vendor(plan: &Plan) -> Option<&Vendor> {
    plan.order_record()
        .expect("the plan writes an Order record")
        .vendor
        .as_ref()
}

/// Whether `plan` reuses the Package's spelling of the Reusable Free-Entry Value in
/// `field` or introduces a new one, as [`reusable`] spells it.
pub fn planned_reuse(plan: &Plan, field: &str) -> serde_json::Value {
    use crate::values::{ReusableValue, ReusableValueKind};
    let values = &plan
        .order_record()
        .expect("the plan writes an Order record")
        .values;
    let kind = ReusableValueKind::ALL
        .into_iter()
        .find(|kind| kind.field() == field)
        .unwrap_or_else(|| panic!("{field} is not a Reusable Free-Entry Value"));
    match values.of(kind) {
        ReusableValue::Absent => serde_json::json!({ "kind": "absent" }),
        ReusableValue::Reused(value) => reusable("reused", value),
        ReusableValue::New(value) => reusable("new", value),
    }
}

/// The Vendor record `plan` would write.
pub fn planned_vendor(plan: &Plan) -> &Vendor {
    plan.vendor_record()
        .expect("the plan writes a Vendor record")
}

/// The facts a refused commit says no longer stand, as `(name, change)` pairs.
///
/// The refusal is only useful if it says *which* fact moved, so every test below
/// reads the names rather than the rejection's variant alone.
pub fn unconfirmed_facts(
    report: &Result<Report, Failure>,
) -> Vec<(&str, crate::evidence::FactChange)> {
    let Err(Failure::PlanEvidenceUnconfirmed { outcome }) = report else {
        panic!("a plan whose evidence moved is refused before any effect: {report:?}");
    };
    let crate::evidence::EvidenceOutcome::Changed(stale) = outcome else {
        panic!("the refusal names the evidence that changed: {outcome:?}");
    };
    stale
        .changed
        .iter()
        .map(|fact| (fact.name.as_str(), fact.change))
        .collect()
}

/// Every finding `diagnosis` reports, in its groups' order.
pub fn findings(diagnosis: &Diagnosis) -> impl Iterator<Item = &Diagnostic> {
    diagnosis
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
}

/// Whether no finding `diagnosis` reports carries error severity.
pub fn valid(diagnosis: &Diagnosis) -> bool {
    findings(diagnosis).all(|finding| finding.severity != Severity::Error)
}

/// The repair case `diagnosis` offers under `handle`.
pub fn case<'a>(diagnosis: &'a Diagnosis, handle: &str) -> Option<&'a RepairCase> {
    diagnosis.cases.iter().find(|case| case.handle == handle)
}

/// A Location a test expects inside a Package: an absolute path and the Package's own
/// spelling for it.
#[derive(Clone, Debug)]
pub struct Spelled {
    pub absolute_path: PathBuf,
    pub package_relative_path: String,
}

impl PartialEq<Spelled> for Location {
    fn eq(&self, expected: &Spelled) -> bool {
        self.absolute_path == expected.absolute_path
            && self.package_relative_path.as_deref()
                == Some(expected.package_relative_path.as_str())
    }
}

/// The Location a test expects at `absolute_path`, which the Package spells
/// `package_relative_path`.
pub fn location(absolute_path: &str, package_relative_path: &str) -> Spelled {
    Spelled {
        absolute_path: absolute_path.into(),
        package_relative_path: package_relative_path.into(),
    }
}

/// The Location a failure names, where the Package could spell the path as one.
pub fn failed_location(path: &FailedPath) -> Option<&Location> {
    match path {
        FailedPath::Located(location) => Some(location),
        FailedPath::Unlocated(_) => None,
    }
}

/// The path a failure names, however well the Package could spell it.
pub fn failed_path(path: &FailedPath) -> &Path {
    match path {
        FailedPath::Located(location) => &location.absolute_path,
        FailedPath::Unlocated(path) => path,
    }
}

/// What staging evidence reports of a candidate: whether its destination is valid, and the
/// digest naming the candidate's exact bytes.
pub fn staging_evidence(evidence: &DiagnosticEvidence) -> (Option<bool>, Option<String>) {
    match evidence {
        DiagnosticEvidence::StagingArtifactPresent {
            destination_valid,
            candidate_digest,
        } => (
            *destination_valid,
            candidate_digest.as_ref().map(ToString::to_string),
        ),
        other => panic!("{other:?} is not staging evidence"),
    }
}

/// The lowercase hexadecimal SHA-256 of `bytes`, which is how evidence names exact
/// content, so it agrees with any other tool that digests the same file.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The plan `change` makes, which the test expects it to make.
pub fn planned<F: FileSystem>(change: &Change, effects: &TestEffects<'_, F>) -> Plan {
    plan(change, effects)
        .result
        .unwrap_or_else(|failure| panic!("{change:?} plans: {failure:?}"))
}

/// The result `change` commits, which the test expects it to commit without warning.
pub fn committed<F: FileSystem>(change: &Change, effects: &TestEffects<'_, F>) -> ChangeResult {
    match commit(planned(change, effects), effects).result {
        Ok(Report::Committed { result, .. }) => result,
        other => panic!("{change:?} commits: {other:?}"),
    }
}

/// A Package whose minimal index agrees with its canonical records.
pub fn proven_current_minimal_package() -> ScriptedFileSystem {
    let filesystem = minimal_package();
    filesystem.put_file(
        "/package/.spielorders/index.json",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../contracts/fixtures/derived-index/minimal-deterministic.json"
        )),
    );
    filesystem
}

/// A created Order at a known identity, returning its Order Directory.
pub fn created_order(root: &Path, uuid: &str, assignments: Assignments) -> PathBuf {
    let result = committed(
        &Change::CreateOrder {
            root: root.into(),
            assignments,
            items: Vec::new(),
        },
        &effects(&PlatformFileSystem).minting(FixedUuid(uuid.into())),
    );
    variant!(result, ChangeResult::CreateOrder)
        .directory
        .absolute_path
        .to_path_buf()
}

const MINIMAL_VERSION: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/version.json"
));
const MINIMAL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/vendors.json"
));

/// A private volume holding a minimal current Package at `/package`: its managed
/// artifacts, no Vendors, and an empty Orders directory.
///
/// The volume offers no change evidence until a test supplies some, so an index is
/// proven current only where the test says it is.
pub fn minimal_package() -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::private("minimal-package")
        .with_file("/package/.spielorders/config.toml", b"")
        .with_file("/package/.spielorders/version.json", MINIMAL_VERSION)
        .with_file("/package/.spielorders/vendors.json", MINIMAL_VENDORS)
        .with_directory("/package/Orders");
    filesystem
        .on(Operation::ObserveFileChange)
        .reports_evidence(FileChangeEvidence::Uncertain);
    filesystem
}

/// Has the minimal Package's Orders directory list these Order Directories in exactly
/// this order, so a test can show a result does not depend on enumeration order.
pub fn enumerate_orders_as(filesystem: &ScriptedFileSystem, names: &[&str]) {
    filesystem
        .on(Operation::ReadDirectory)
        .at("/package/Orders")
        .answers(Answer::Listing(
            names
                .iter()
                .map(|name| DirectoryEntry {
                    name: name.into(),
                    kind: FileSystemEntryKind::Directory,
                })
                .collect(),
        ));
}

/// The initialization of the Package at `root` with its Orders and Removed Orders
/// directories at `layout`.
pub fn initialization(
    root: impl Into<PathBuf>,
    (orders_path, removed_path): &(&str, &str),
) -> Change {
    Change::InitializePackage {
        root: root.into(),
        orders_path: Some((*orders_path).to_owned()),
        removed_path: Some((*removed_path).to_owned()),
    }
}

/// A committed Package at `layout`, in a tree of its own.
pub fn package_tree_with(label: &str, layout: &(&str, &str)) -> TemporaryTree {
    let tree = TemporaryTree::new(label);
    committed(
        &initialization(tree.path(), layout),
        &effects(&PlatformFileSystem),
    );
    tree
}

/// A committed Package at its default layout, in a tree of its own.
pub fn package_tree(label: &str) -> TemporaryTree {
    let tree = TemporaryTree::new(label);
    committed(&initialize(tree.path()), &effects(&PlatformFileSystem));
    tree
}

/// A committed Package at its default layout, and its canonical root.
pub fn initialized_package(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = package_tree(label);
    let root = std::fs::canonicalize(tree.path()).unwrap();
    (tree, root)
}

/// A committed Vendor named `name` at a known identity, returning that identity.
pub fn created_vendor(root: &Path, name: &str, vendor_id: &str) -> String {
    let result = committed(
        &Change::CreateVendor {
            root: root.into(),
            assignments: Assignments::new().assign("name", name),
            contacts: Vec::new(),
        },
        &effects(&PlatformFileSystem).minting(FixedUuid(vendor_id.into())),
    );
    variant!(result, ChangeResult::Vendor)
        .vendor_id
        .deref()
        .to_owned()
}

/// One adopted Order at a known identity, for tests that need a real canonical Order
/// rather than a hand-written record.
pub fn adopted_order(root: &Path, basename: &str, uuid: &str) -> std::path::PathBuf {
    let directory = root.join("Orders").join(basename);
    std::fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    committed(
        &Change::AdoptOrder {
            root: root.into(),
            directory: directory.clone(),
            assignments: Assignments::new()
                .assign("title", "Laser components")
                .assign("date_ordered", "2026-08-22")
                .assign("status", "hold"),
            items: Vec::new(),
        },
        &effects(&PlatformFileSystem).minting(FixedUuid(uuid.into())),
    );
    directory
}

/// An Order Directory's canonical record.
pub fn record_path(directory: &Path) -> std::path::PathBuf {
    directory.join(".spielorders/order.json")
}

/// The handle the current diagnosis gives the case carrying `code`.
///
/// Taken from diagnosis rather than reconstructed, so a test consumes the published
/// contract instead of restating how a handle is built.
pub fn diagnosed_handle(directory: &Path, code: DiagnosticCode) -> String {
    let diagnosis = diagnosed_order(directory, &PlatformFileSystem)
        .result
        .expect("the Order diagnoses");
    diagnosis
        .cases
        .iter()
        .find(|case| case.code == code)
        .unwrap_or_else(|| panic!("{code:?} has a repair case: {:?}", diagnosis.cases))
        .handle
        .to_owned()
}

/// Where an outside writer's edit lands during an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditPoint {
    /// At the next staging write — after the operation observed its destination and
    /// before it renames over it. This is the window a destination guard closes.
    StagingWrite,
    /// Just after the next read of a path ending in this suffix, for operations whose
    /// own reads are the only port events between observing and writing.
    ReadEndingIn(&'static str),
}

/// Real effects, except that one writer outside the operation under test replaces
/// `path` partway through it. [`ScriptedFileSystem::disturbed`] says whether it did.
pub fn another_writer_replacing(
    path: &Path,
    replacement: &[u8],
    at: EditPoint,
) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    let (path, replacement) = (path.to_path_buf(), replacement.to_vec());
    let write = move |outside: &Outside<'_>| outside.write(&path, &replacement);
    match at {
        EditPoint::StagingWrite => filesystem
            .on(Operation::WriteNew)
            .once()
            .preceded_by(write)
            .proceeds(),
        EditPoint::ReadEndingIn(suffix) => filesystem
            .on(Operation::Read)
            .ending_in(suffix)
            .once()
            .followed_by(write)
            .proceeds(),
    }
    filesystem
}

/// Has something outside SpielOrders remove an Order record the moment an adoption
/// links it into place, so the adoption really happened and its refresh cannot see it.
pub fn record_vanishing_after_adoption(filesystem: &ScriptedFileSystem) {
    filesystem
        .on(Operation::LinkNewFile)
        .ending_in(".spielorders/order.json")
        .once()
        .followed_by(|outside| outside.remove_file(&outside.call().path))
        .proceeds();
}

/// Where the replacement of one managed file — an Order record, the Vendor aggregate,
/// or a README — is made to fail. Every other effect is real.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedFileBoundary {
    ReadRecord,
    WriteStage,
    SyncStage,
    Replace,
    IndexReplacement,
    /// The file is replaced, and the refresh that follows cannot list a directory.
    RefreshListing,
    /// The file is replaced, and the Package is damaged before the refresh loads it.
    PackageDamagedInRefresh,
}

/// The staging file a replacement of `file` writes beside it.
pub fn stage_of(file: &str) -> String {
    let file = Path::new(file);
    let name = file.file_name().expect("a managed file has a name");
    let stage = format!(".{}.spielorders-stage", name.to_string_lossy());
    file.with_file_name(stage).to_string_lossy().into_owned()
}

/// Real effects, except that the replacement of the managed file ending in `file`
/// fails once at `boundary`.
pub fn failing_replacement(file: &str, boundary: ManagedFileBoundary) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    let stage = stage_of(file);
    let index = ".spielorders/index.json";
    match boundary {
        ManagedFileBoundary::ReadRecord => filesystem
            .on(Operation::Read)
            .ending_in(file)
            .once()
            .fails(),
        ManagedFileBoundary::WriteStage => filesystem
            .on(Operation::WriteNew)
            .ending_in(stage)
            .once()
            .fails(),
        ManagedFileBoundary::SyncStage => filesystem
            .on(Operation::SyncFile)
            .ending_in(stage)
            .once()
            .fails(),
        ManagedFileBoundary::Replace => filesystem
            .on(Operation::ReplaceFile)
            .ending_in(file)
            .once()
            .fails(),
        ManagedFileBoundary::IndexReplacement => filesystem
            .on(Operation::ReplaceFile)
            .ending_in(index)
            .once()
            .fails(),
        ManagedFileBoundary::RefreshListing => breaking_refresh_after(
            &filesystem,
            Operation::ReplaceFile,
            file,
            RefreshBreakage::ListingFails,
        ),
        ManagedFileBoundary::PackageDamagedInRefresh => breaking_refresh_after(
            &filesystem,
            Operation::ReplaceFile,
            file,
            RefreshBreakage::PackageDamaged,
        ),
    }
    filesystem
}

/// [`failing_replacement`], with every removal of the file's stage failing too, so a
/// staging file is retained and reported.
pub fn with_stage_cleanup_failure(file: &str, boundary: ManagedFileBoundary) -> ScriptedFileSystem {
    let filesystem = failing_replacement(file, boundary);
    filesystem
        .on(Operation::RemoveFile)
        .ending_in(stage_of(file))
        .fails();
    filesystem
}

/// What goes wrong in the Package load that refreshes the index after a mutation
/// has committed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshBreakage {
    /// The refresh cannot list a directory.
    ListingFails,
    /// An Order record no longer parses, so the Order reloads as a Damaged Order.
    RecordDamaged,
    /// The Package's version marker no longer parses, so the refresh returns no
    /// current Package at all.
    PackageDamaged,
}

/// Breaks the refresh that follows a mutation's decisive commit — the first call to
/// `decisive` whose path ends in `suffix`, where an empty suffix matches any path —
/// and leaves every planning and commit effect before it real.
pub fn breaking_refresh_after(
    filesystem: &ScriptedFileSystem,
    decisive: Operation,
    suffix: &str,
    breakage: RefreshBreakage,
) {
    let script = match breakage {
        RefreshBreakage::ListingFails => filesystem.on(Operation::ReadDirectory),
        RefreshBreakage::RecordDamaged => filesystem
            .on(Operation::Read)
            .ending_in(".spielorders/order.json"),
        RefreshBreakage::PackageDamaged => filesystem
            .on(Operation::Read)
            .ending_in(".spielorders/version.json"),
    }
    .after(decisive, suffix);
    match breakage {
        RefreshBreakage::ListingFails => script.fails(),
        _ => script.answers(Answer::Bytes(b"{ not json".to_vec())),
    }
}

/// The original a writer that read `path` just now would carry: its absence, its
/// exact text bytes, or no established content when the shipping reader cannot read it.
pub fn readme_original(path: &Path) -> crate::readme_writes::ReadmeOriginal {
    use crate::readme_writes::ReadmeOriginal;
    let directory = path.parent().expect("a README is inside a directory");
    let effects = effects(&PlatformFileSystem);
    crate::read_readme(
        &Scope::Order(OrderRef::Directory(directory.into())),
        &effects,
    )
    .result
    .or_else(|_| crate::read_readme(&Scope::Package(directory.into()), &effects).result)
    .map(|read| ReadmeOriginal::from(&read))
    .unwrap_or(ReadmeOriginal::Unavailable)
}

/// [`readme_original`] of the README directly inside `directory`, a Package Root or an
/// Order Directory.
pub fn current_readme(directory: &Path) -> crate::readme_writes::ReadmeOriginal {
    readme_original(&directory.join("README.md"))
}

/// A volume that behaves as FAT or exFAT does: it keeps no comparable change evidence,
/// and it has no exclusive rename.
pub fn exfat_like() -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    filesystem
        .on(Operation::ObserveFileChange)
        .reports_evidence(crate::ports::FileChangeEvidence::Uncertain);
    filesystem
        .on(Operation::RenameNew)
        .fails_with(std::io::ErrorKind::Unsupported);
    filesystem
}

/// What the interface reports of `failure`: each entry's code and Package-relative
/// path, in order.
pub fn reported(failure: &Failure) -> Vec<(crate::SpielordersConstant, String)> {
    let mut entries = Vec::new();
    crate::Entry::failures(failure, crate::Subject::Other, &mut entries);
    entries
        .into_iter()
        .map(|entry| (entry.code, entry.path.to_string_lossy().into_owned()))
        .collect()
}

/// What the interface reports of `warning`: each entry's code and Package-relative path,
/// in order.
pub fn warning_entries(warning: &crate::Warning) -> Vec<(crate::SpielordersConstant, String)> {
    let mut entries = Vec::new();
    crate::Entry::warnings(warning, &mut entries);
    entries
        .into_iter()
        .map(|entry| (entry.code, entry.path.to_string_lossy().into_owned()))
        .collect()
}

/// `location` as the interface spells it.
pub fn spelled(location: &Location) -> String {
    location
        .package_relative_path
        .clone()
        .unwrap_or_else(|| location.absolute_path.to_string_lossy().into_owned())
}

/// Asserts the interface reports `failure`, a stopped commit, as its initiating entry at
/// where the commit stopped, then one entry beside it for each artifact its cleanup left
/// behind, each at its own location.
pub fn assert_reported_beside(failure: &Failure) {
    let code = if failure.cleanup_failures().is_empty() {
        crate::SPIELORDERS_FAILURE_COMMIT_FAILED
    } else {
        crate::SPIELORDERS_FAILURE_COMMIT_FAILED_CLEANUP_INCOMPLETE
    };
    let expected: Vec<_> = std::iter::once(&failure.failure().location)
        .chain(failure.cleanup_failures().iter().map(|left| &left.location))
        .map(|location| (code, spelled(location)))
        .collect();
    assert_eq!(reported(failure), expected, "{failure:?}");
}

/// The parts of a failure a test of a stopped commit expects it to be.
pub trait FailedCommit {
    /// Where the commit stopped.
    fn failure(&self) -> &CommitFailure;
    /// What its cleanup could not remove; empty exactly when it removed everything.
    fn cleanup_failures(&self) -> &[CleanupFailure];
}

impl FailedCommit for Failure {
    fn failure(&self) -> &CommitFailure {
        match self {
            Self::CommitFailed { failure }
            | Self::CommitFailedCleanupIncomplete { failure, .. } => failure,
            other => panic!("a stopped commit fails as one: {other:?}"),
        }
    }

    fn cleanup_failures(&self) -> &[CleanupFailure] {
        match self {
            Self::CommitFailed { .. } => &[],
            Self::CommitFailedCleanupIncomplete {
                cleanup_failures, ..
            } => cleanup_failures,
            other => panic!("a stopped commit fails as one: {other:?}"),
        }
    }
}

/// The one failure a draft that broke one rule refuses with.
pub fn first_fault(failure: Failure) -> Failure {
    match failure {
        Failure::InvalidDraft(faults) if faults.len() == 1 => {
            faults.into_iter().next().expect("one fault").failure
        }
        other => other,
    }
}
