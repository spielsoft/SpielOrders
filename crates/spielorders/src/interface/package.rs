//! An opened Package, and the reads that start from one.

use crate::interface::{
    OptionalText, PathBytes, SPIELORDERS_PACKAGE_STATE_DAMAGED, SPIELORDERS_PACKAGE_STATE_HEALTHY,
    SPIELORDERS_RESULT_BAD_ARGUMENT, SPIELORDERS_RESULT_REFUSED, SPIELORDERS_VENDOR_SELECTOR_ID,
    SPIELORDERS_VENDOR_SELECTOR_NAME, SpielordersConstant, SpielordersEntries, SpielordersOrder,
    SpielordersReadme, SpielordersResult, SpielordersVendor, Text,
    call::{Session, deliver, fallible, quietly, settle},
    codes,
    entries::{EMPTY, Entry, Subject},
    records::{ReadmeScope, path_text},
};
use crate::{
    Failure, OrderRef, Scope, VendorRef,
    diagnosis::package_root_of,
    model::{DamagedPackageProjection, PackageIndexRecord},
    ports::FileSystem,
};
use std::{
    ffi::CString,
    path::{Path, PathBuf},
};

/// One opened Package. A Damaged Package opens in diagnostic-only mode: its lists,
/// reads, drafts, and plans refuse as SPIELORDERS_FAILURE_DAMAGED_PACKAGE, and its
/// diagnosis, validation, README read, and repair work.
#[repr(C)]
#[derive(Clone)]
pub struct SpielordersPackage {
    data: Box<SpielordersPackageData>,
}

#[derive(Clone)]
pub(crate) struct SpielordersPackageData {
    opened: Opened,
    root: PathBuf,
    root_text: CString,
    material_directory: CString,
    findings: Vec<Entry>,
    index: PackageIndexRecord,
}

#[derive(Clone)]
enum Opened {
    Healthy(Box<crate::Package>),
    Damaged(Failure),
}

impl SpielordersPackage {
    fn of(
        answer_root: PathBuf,
        opened: Opened,
        findings: Vec<Entry>,
        index: PackageIndexRecord,
    ) -> Self {
        let material_directory = match &opened {
            Opened::Healthy(package) => package
                .supporting_material_directory()
                .map(path_text)
                .unwrap_or_default(),
            Opened::Damaged(_) => CString::default(),
        };
        Self {
            data: Box::new(SpielordersPackageData {
                opened,
                root_text: path_text(&answer_root),
                material_directory,
                root: answer_root,
                findings,
                index,
            }),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.data.root
    }

    /// Refuses, as SPIELORDERS_FAILURE_ORDER_NOT_FOUND, an Order Directory outside this
    /// Package. A directory the filesystem cannot resolve is left for the call to report.
    pub(crate) fn holds(
        &self,
        directory: &Path,
        session: &Session,
        entries: &mut Vec<Entry>,
    ) -> Result<(), SpielordersResult> {
        let Ok(resolved) = crate::ports::FileSystem::canonicalize(session, directory) else {
            return Ok(());
        };
        if resolved != self.data.root && resolved.starts_with(&self.data.root) {
            // A descendant can belong to a nested Package. Use the same owning-root
            // lookup as diagnosis, including for a damaged Order with no usable locator.
            // If it cannot resolve ownership, the actual read reports that failure.
            match package_root_of(&resolved, session) {
                Ok(root) if root != self.data.root => {}
                _ => return Ok(()),
            }
        }
        let failure = crate::Failure::OrderNotFound {
            directory: crate::operations::FailedPath::of(&resolved),
            root: Some(self.data.root.clone()),
        };
        Entry::failures(&failure, Subject::Order, entries);
        Err(crate::interface::SPIELORDERS_RESULT_REFUSED)
    }

    pub(crate) fn material_directory(&self) -> &std::ffi::CStr {
        &self.data.material_directory
    }

    /// The healthy Package, or the damaged-package refusal as entries.
    pub(crate) fn healthy(
        &self,
        entries: &mut Vec<Entry>,
    ) -> Result<&crate::Package, SpielordersResult> {
        match &self.data.opened {
            Opened::Healthy(package) => Ok(package),
            Opened::Damaged(failure) => {
                Entry::failures(failure, Subject::Other, entries);
                Err(SPIELORDERS_RESULT_REFUSED)
            }
        }
    }
}

fn damaged_findings(package: &DamagedPackageProjection) -> Vec<Entry> {
    package
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .map(Entry::finding)
        .collect()
}

pub(crate) fn open(
    root: &Path,
    session: &Session,
    entries: &mut Vec<Entry>,
) -> Result<SpielordersPackage, SpielordersResult> {
    let answer = crate::open(root, session, session.cancellation());
    match answer.result {
        Err(failure @ (Failure::DamagedPackage(_) | Failure::PackageMetadataUnsupported(_)))
            if !session.is_cancelled() =>
        {
            let (Failure::DamagedPackage(package) | Failure::PackageMetadataUnsupported(package)) =
                &failure
            else {
                unreachable!("matched just above")
            };
            let mut findings: Vec<Entry> = answer.diagnostics.iter().map(Entry::finding).collect();
            findings.extend(damaged_findings(package));
            Ok(SpielordersPackage::of(
                package.context.root.absolute_path.clone(),
                Opened::Damaged(failure.clone()),
                findings,
                package.index.clone(),
            ))
        }
        result => {
            let package = settle(
                crate::Answer {
                    result,
                    diagnostics: Vec::new(),
                },
                session,
                Subject::Other,
                entries,
            )?;
            let mut findings: Vec<Entry> = answer.diagnostics.iter().map(Entry::finding).collect();
            findings.extend(
                package
                    .contents
                    .diagnostic_groups
                    .iter()
                    .flat_map(|group| &group.diagnostics)
                    .map(Entry::finding),
            );
            findings.extend(package.readme.diagnostics.iter().map(Entry::finding));
            let index = package.contents.index.clone();
            Ok(SpielordersPackage::of(
                package.contents.context.root.absolute_path.clone(),
                Opened::Healthy(Box::new(package)),
                findings,
                index,
            ))
        }
    }
}

/// Opens the Package at `root`, writing nothing. Refuses a root that is a link before
/// reading anything behind it, and a folder that is not a SpielOrders Root. A Damaged
/// Package opens with state SPIELORDERS_PACKAGE_STATE_DAMAGED.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_open(
    root: PathBytes<'_>,
    token: u64,
    package: &mut Option<Box<SpielordersPackage>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(package, None);
    fallible(outcome, |entries| {
        let session = Session::new(token);
        deliver(open(root.as_path(), &session, entries), package)
    })
}

/// Opens the Package an Order Directory belongs to, found through the Order's stored
/// locator. A Damaged Order Directory, including one whose record or locator is what is
/// damaged, opens the Package that holds it, where it can be validated and repaired.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_open_from_order(
    order_directory: PathBytes<'_>,
    token: u64,
    package: &mut Option<Box<SpielordersPackage>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(package, None);
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let read = crate::read_order(
            &OrderRef::Directory(order_directory.as_path().to_path_buf()),
            &session,
        );
        let holding = || {
            session
                .canonicalize(order_directory.as_path())
                .ok()
                .and_then(|directory| package_root_of(&directory, &session).ok())
        };
        let root = match &read.result {
            Err(Failure::DamagedOrder { context, .. }) => Ok(context.root.absolute_path.clone()),
            Err(
                Failure::DamagedRecord { .. }
                | Failure::LocatorUnusable { .. }
                | Failure::LocatorReachesNoPackageHoldingThisOrder { .. },
            ) if let Some(root) = holding() => Ok(root),
            _ => settle(read, &session, Subject::Order, entries)
                .map(|read| read.context.root.absolute_path),
        };
        deliver(
            root.and_then(|root| open(&root, &session, entries)),
            package,
        )
    })
}

/// The `SPIELORDERS_PACKAGE_STATE_*` of `package`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_state(package: &SpielordersPackage) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || match package.data.opened {
        Opened::Healthy(_) => SPIELORDERS_PACKAGE_STATE_HEALTHY,
        Opened::Damaged(_) => SPIELORDERS_PACKAGE_STATE_DAMAGED,
    })
}

/// The Root of `package`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_root(package: &SpielordersPackage) -> PathBytes<'_> {
    quietly(PathBytes::new(EMPTY), || {
        PathBytes::new(&package.data.root_text)
    })
}

/// The findings opening `package` made, as a new Entries handle the caller frees, or
/// NULL when there are none. Findings never change a call's result.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_findings(
    package: &SpielordersPackage,
) -> Option<Box<SpielordersEntries>> {
    quietly(None, || {
        SpielordersEntries::clone_of(&package.data.findings)
    })
}

/// The `SPIELORDERS_INDEX_STATE_*` of the stored Package Index, as opening assessed it
/// without writing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_index_state(
    package: &SpielordersPackage,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        codes::index_state(package.data.index.state)
    })
}

fn index_reasons(index: &PackageIndexRecord) -> Vec<SpielordersConstant> {
    index
        .stale_reasons
        .iter()
        .copied()
        .map(codes::stale_reason)
        .chain(
            index
                .malformed_reasons
                .iter()
                .copied()
                .map(codes::malformed_reason),
        )
        .collect()
}

/// How many reasons the stored Package Index is stale or malformed for.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_index_reason_count(package: &SpielordersPackage) -> i64 {
    quietly(-1, || {
        i64::try_from(index_reasons(&package.data.index).len()).unwrap_or(-1)
    })
}

/// Reason `index` the stored Package Index is stale or malformed: a
/// `SPIELORDERS_INDEX_REASON_*` constant, or -1 when out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_index_reason(
    package: &SpielordersPackage,
    index: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        usize::try_from(index)
            .ok()
            .and_then(|index| index_reasons(&package.data.index).get(index).copied())
            .unwrap_or(SpielordersConstant::NONE)
    })
}

/// Reads afresh the healthy Order at `order_directory`, one Order Directory of `package`,
/// with the findings local to it. A directory outside `package` refuses
/// SPIELORDERS_FAILURE_ORDER_NOT_FOUND. The read is the evidence a draft of it saves
/// against.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_read_order(
    package: &SpielordersPackage,
    order_directory: PathBytes<'_>,
    token: u64,
    order: &mut Option<Box<SpielordersOrder>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(order, None);
    fallible(outcome, |entries| {
        let directory = OrderRef::Directory(order_directory.as_path().to_path_buf());
        let result = package.healthy(entries).and_then(|_| {
            let session = Session::new(token);
            package.holds(order_directory.as_path(), &session, entries)?;
            let answer = crate::read_order(&directory, &session);
            settle(answer, &session, Subject::Order, entries)
        });
        if let Ok(read) = &result {
            // The README's own diagnostics are among the Order's wherever the load read it.
            let readme = read
                .readme
                .diagnostics
                .iter()
                .filter(|diagnostic| !read.diagnostics.contains(diagnostic));
            entries.extend(read.diagnostics.iter().chain(readme).map(Entry::finding));
        }
        deliver(result.map(SpielordersOrder::of), order)
    })
}

/// Reads one Vendor afresh, Hidden or not. `selector` is a
/// `SPIELORDERS_VENDOR_SELECTOR_*` constant saying whether `text` is its ID or its Name.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_read_vendor(
    package: &SpielordersPackage,
    selector: SpielordersConstant,
    text: Text<'_>,
    token: u64,
    vendor: &mut Option<Box<SpielordersVendor>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(vendor, None);
    fallible(outcome, |entries| {
        let Some(text) = text.to_str() else {
            return deliver(Err(SPIELORDERS_RESULT_BAD_ARGUMENT), vendor);
        };
        let named = match selector {
            SPIELORDERS_VENDOR_SELECTOR_ID => VendorRef::Id(text.to_owned()),
            SPIELORDERS_VENDOR_SELECTOR_NAME => VendorRef::Name(text.to_owned()),
            _ => return deliver(Err(SPIELORDERS_RESULT_BAD_ARGUMENT), vendor),
        };
        let result = package.healthy(entries).and_then(|_| {
            let session = Session::new(token);
            let answer = crate::read_vendor(package.root(), &named, &session);
            settle(answer, &session, Subject::Vendor, entries)
        });
        if let Ok(read) = &result {
            entries.extend(read.diagnostics.iter().map(Entry::finding));
        }
        deliver(result.map(SpielordersVendor::of), vendor)
    })
}

/// Reads the Package README afresh. Works on a Damaged Package.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_read_readme(
    package: &SpielordersPackage,
    token: u64,
    readme: &mut Option<Box<SpielordersReadme>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(readme, None);
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let root = package.root().to_path_buf();
        let result = settle(
            crate::read_readme(&Scope::Package(root.clone()), &session),
            &session,
            Subject::Other,
            entries,
        );
        if let Ok(read) = &result {
            entries.extend(read.diagnostics.iter().map(Entry::finding));
        }
        deliver(
            result.map(|read| SpielordersReadme::of(read, ReadmeScope::Package(root))),
            readme,
        )
    })
}

/// Frees `package`. Freeing NULL does nothing; handles read from it stay valid.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_free(package: Option<Box<SpielordersPackage>>) {
    quietly((), || drop(package));
}

/// The text, or "" for NULL; BAD_ARGUMENT when it is not UTF-8.
pub(crate) fn text_or_empty(text: OptionalText<'_>) -> Result<String, SpielordersResult> {
    match text.as_c_str() {
        None => Ok(String::new()),
        Some(text) => text
            .to_str()
            .map(str::to_owned)
            .map_err(|_| SPIELORDERS_RESULT_BAD_ARGUMENT),
    }
}
