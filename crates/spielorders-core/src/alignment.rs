//! Planning and persistence for one explicit Order Directory alignment.
//!
//! Alignment is the only operation that renames an Order Directory. Fact edits,
//! status changes, and Vendor changes deliberately leave directory drift in place,
//! so a caller must ask for this explicitly.

use crate::{
    canonical::{
        CurrentPackageCandidate, HealthyOrder, PackageIndexRecord, PackageLoad, open_package,
    },
    creation::{normalize_filename_component, projected_order_basename},
    evidence::{EvidenceFingerprint, EvidenceOutcome},
    metadata::{OrderDocument, VendorId},
    mutation::MutationReport,
    operations::{FailedPath, OperationFailure},
    order_edits::{OrderResolutionFailure, resolve_order},
    ports::{DirectoryFileSystem, FileSystem, FileSystemEntryKind, NewDirectoryRenameOutcome},
    readme::{Location, package_relative_path_text},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderAlignmentOperationKind {
    RenameOrderDirectory,
    RefreshPackageIndex,
}

impl OrderAlignmentOperationKind {
    /// This operation's one stable spelling.
    ///
    /// Beside the operation rather than in an adapter, so an operation reported by the CLI
    /// and the same operation reported to the desktop shell cannot come to be named
    /// differently.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RenameOrderDirectory => "rename_order_directory",
            Self::RefreshPackageIndex => "refresh_package_index",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderAlignmentOperation {
    kind: OrderAlignmentOperationKind,
    location: Location,
}

impl OrderAlignmentOperation {
    pub fn kind(&self) -> OrderAlignmentOperationKind {
        self.kind
    }

    /// Where this operation acts, as this Package locates it — so a caller reporting it
    /// carries the Package-relative spelling as well as the absolute path.
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn path(&self) -> &Path {
        self.location.absolute_path()
    }
}

/// One complete, validated alignment of one healthy Order Directory.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderAlignmentPlan {
    root: PathBuf,
    source: Location,
    destination: Location,
    basename: String,
    document: OrderDocument,
    readme_text: Option<String>,
    prior_index: PackageIndexRecord,
    evidence: EvidenceFingerprint,
    operations: Vec<OrderAlignmentOperation>,
}

impl OrderAlignmentPlan {
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The Order Directory as it stands now, as this Package locates it.
    ///
    /// A `Location` rather than a path, for the reason
    /// [`OrderAlignmentOperation::location`] carries one: a caller reporting where this
    /// directory is carries the Package-relative spelling as well as the absolute path,
    /// and the normalized Package-relative grammar is this crate's.
    pub fn source(&self) -> &Location {
        &self.source
    }

    /// The Order Directory this alignment would rename it to, as this Package locates it.
    pub fn destination(&self) -> &Location {
        &self.destination
    }

    /// The exact projected basename from the Order's current facts.
    pub fn basename(&self) -> &str {
        &self.basename
    }

    pub fn order_id(&self) -> &str {
        self.document.order_id.as_str()
    }

    pub fn operations(&self) -> &[OrderAlignmentOperation] {
        &self.operations
    }

    /// The derived Package Index as it stood when this alignment was planned.
    ///
    /// The state a run that never commits has to report: committing refreshes the index
    /// and [`OrderAlignmentResult::index`] is that refreshed record, so a dry run has
    /// none of its own and answers with what it observed instead.
    pub fn prior_index(&self) -> &PackageIndexRecord {
        &self.prior_index
    }

    /// Whether committing this plan would rename the directory. Drift is ordinary
    /// allowed state, so an already aligned Order plans no operation at all.
    pub fn changes_directory_name(&self) -> bool {
        self.source.absolute_path() != self.destination.absolute_path()
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderAlignmentPlanError<E> {
    UnmanagedPackage,
    DamagedPackage,
    UpgradePending,
    PackageOperationFailed,
    OrderNotFound {
        path: PathBuf,
    },
    DamagedOrder {
        path: PathBuf,
    },
    /// The Order's facts project a basename with an empty required component, so no
    /// aligned name exists to rename to.
    EmptyProjectedComponent,
    /// Something already occupies the projected name. Alignment stops rather than
    /// inventing a suffix.
    ///
    /// **A `Location` rather than a `PathBuf`**, because the projected destination is a
    /// path inside this Package and [`located`] has already established its
    /// Package-relative spelling — which a refusal owes exactly as a success does.
    DestinationCollision {
        location: Location,
    },
    UnresolvedVendor {
        vendor_id: String,
    },
    /// **A [`FailedPath`] rather than a `PathBuf`**, because the paths that reach here do
    /// not agree about whether a Package surrounds them: an operation inside a Root this
    /// capability established carries that Root's spelling, and one performed while
    /// resolving the Order Directory up to its Package happens before there is a Root to
    /// spell it against. The type is what tells them apart.
    FileSystem {
        path: FailedPath,
        error: E,
    },
}

impl<E> From<OrderResolutionFailure<E>> for OrderAlignmentPlanError<E> {
    fn from(failure: OrderResolutionFailure<E>) -> Self {
        match failure {
            OrderResolutionFailure::UnmanagedPackage => Self::UnmanagedPackage,
            OrderResolutionFailure::DamagedPackage => Self::DamagedPackage,
            OrderResolutionFailure::UpgradePending => Self::UpgradePending,
            OrderResolutionFailure::PackageOperationFailed => Self::PackageOperationFailed,
            OrderResolutionFailure::OrderNotFound { path } => Self::OrderNotFound { path },
            OrderResolutionFailure::DamagedOrder { path } => Self::DamagedOrder { path },
            OrderResolutionFailure::FileSystem { path, error } => Self::FileSystem { path, error },
        }
    }
}

pub fn plan_order_alignment<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<OrderAlignmentPlan, OrderAlignmentPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let document = order.document().clone();
    let basename = projected_basename(&document, &package)?;

    let root = package.context().root().absolute_path().to_path_buf();
    let source = order.paths().directory().clone();
    let parent = source
        .absolute_path()
        .parent()
        .ok_or_else(|| OrderAlignmentPlanError::OrderNotFound {
            path: source.absolute_path().to_path_buf(),
        })?
        .to_path_buf();
    let destination = located(&root, parent.join(&basename));
    let index = package.context().index().clone();

    let operations = if source.absolute_path() == destination.absolute_path() {
        Vec::new()
    } else {
        // The projected name must be free. A collision stops alignment; it never
        // invents a suffix and never replaces whatever is already there.
        match filesystem.symlink_metadata(destination.absolute_path()) {
            Ok(None) => {}
            Ok(Some(_)) => {
                return Err(OrderAlignmentPlanError::DestinationCollision {
                    location: destination.clone(),
                });
            }
            Err(error) => {
                return Err(OrderAlignmentPlanError::FileSystem {
                    path: FailedPath::Located(destination.clone()),
                    error,
                });
            }
        }
        vec![
            OrderAlignmentOperation {
                kind: OrderAlignmentOperationKind::RenameOrderDirectory,
                location: destination.clone(),
            },
            OrderAlignmentOperation {
                kind: OrderAlignmentOperationKind::RefreshPackageIndex,
                location: index,
            },
        ]
    };

    let evidence = alignment_evidence(&package, &order);
    Ok(OrderAlignmentPlan {
        root,
        source,
        destination,
        basename,
        document,
        // An alignment moves the README with its directory, so what the load knew
        // before the rename is what the renamed Order holds.
        readme_text: order.readme_text().map(str::to_owned),
        prior_index: package.index().clone(),
        evidence,
        operations,
    })
}

/// Projects the aligned basename from the Order's current facts and its live Vendor
/// Name. An Order retains no historical Vendor snapshot, so the name follows the
/// Vendor record as it stands now.
fn projected_basename<E>(
    document: &OrderDocument,
    package: &CurrentPackageCandidate,
) -> Result<String, OrderAlignmentPlanError<E>> {
    let vendor_name = match document.vendor_id.as_ref() {
        Some(vendor_id) => Some(resolved_vendor_name(vendor_id, package).ok_or_else(|| {
            OrderAlignmentPlanError::UnresolvedVendor {
                vendor_id: vendor_id.as_str().to_owned(),
            }
        })?),
        None => None,
    };
    let title_component = normalize_filename_component(document.title.as_str());
    if title_component.is_empty() {
        return Err(OrderAlignmentPlanError::EmptyProjectedComponent);
    }
    Ok(projected_order_basename(
        document.date_ordered.as_str(),
        vendor_name,
        &title_component,
        document.orderer.as_deref(),
    ))
}

/// The current Name of one referenced Vendor, resolved live from the Package. `None` is
/// an Unresolved Vendor Reference, which is an integrity failure and not an Order
/// without a Vendor; callers distinguish the two by whether a reference exists at all.
fn resolved_vendor_name<'a>(
    vendor_id: &VendorId,
    package: &'a CurrentPackageCandidate,
) -> Option<&'a str> {
    package
        .vendors()
        .iter()
        .find(|vendor| vendor.vendor_id == *vendor_id)
        .map(|vendor| vendor.name.as_str())
}

/// The path an alignment of `source` would rename it to, projected from the Order's
/// facts as they stand in `package`. `None` when those facts project no aligned name
/// at all, which is a refusal and never a rename to some fallback.
fn projected_destination(
    source: &Path,
    document: &OrderDocument,
    package: &CurrentPackageCandidate,
) -> Option<PathBuf> {
    // The same projection the plan used, so plan-time and commit-time answers cannot
    // drift apart through two spellings of one rule.
    let basename = projected_basename::<()>(document, package).ok()?;
    Some(source.parent()?.join(basename))
}

/// A path as its exact bytes. A path is not required to be UTF-8, and a comparison
/// that silently repaired one would lose exactly the difference this seam looks for.
fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

/// The facts an alignment plan is authorized against, as `order` and `package` stand
/// now.
///
/// Alignment's one effect is a rename, decided by *which* Order is being renamed and
/// *what* name its current facts project. Neither is visible in the path, so neither
/// is covered by confirming that the path still exists: `symlink_metadata` reports an
/// ordinary directory whether the Order standing there is the planned one or a
/// different one left in its place, and an ordinary fact edit or Vendor rename
/// deliberately leaves the directory alone while making the plan's projected name the
/// wrong one. Each fact is recorded here and re-observed through
/// [`EvidenceFingerprint::confirm`] immediately before that rename.
fn alignment_evidence(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
) -> EvidenceFingerprint {
    let document = order.document();
    // Read from the resolved Order rather than taken from the plan, because comparing
    // the plan's own copy of its source against itself would confirm nothing. Identity
    // is what separates one Order from another here; this fact carries the location the
    // destination below is projected from.
    let source = order.paths().directory().absolute_path();
    let fingerprint = EvidenceFingerprint::new()
        // Identity, because the path is location and not identity: an Order ID is what
        // separates this Order from any other that could stand at the same path.
        .observed("order_id", document.order_id.as_str())
        .observed("source", path_bytes(source))
        // The Package the source belongs to, resolved through the Order's own locator.
        // An Order that has come to belong somewhere else is not this plan's subject
        // even where its identity and its path both still match.
        .observed(
            "package_root",
            path_bytes(package.context().root().absolute_path()),
        )
        // Every fact the basename projects from, each under its own name, so a refusal
        // says which one moved rather than only that the projected name would differ.
        .observed("title", document.title.as_str())
        .observed("date_ordered", document.date_ordered.as_str());
    let fingerprint = match document.orderer.as_deref() {
        Some(orderer) => fingerprint.observed("orderer", orderer),
        None => fingerprint.absent("orderer"),
    };
    let fingerprint = match document.vendor_id.as_ref() {
        Some(vendor_id) => fingerprint.observed("vendor_id", vendor_id.as_str()),
        None => fingerprint.absent("vendor_id"),
    };
    // The live Vendor Name, not the reference alone. An Order keeps no Vendor snapshot,
    // so renaming the Vendor changes the name this Order projects while every Order
    // fact, the Vendor ID, and the source path all still stand.
    // An unresolved reference records a present `vendor_id` with no name, which no
    // absent reference can be mistaken for.
    let fingerprint = match document
        .vendor_id
        .as_ref()
        .and_then(|vendor_id| resolved_vendor_name(vendor_id, package))
    {
        Some(name) => fingerprint.observed("vendor_name", name),
        None => fingerprint.absent("vendor_name"),
    };
    // The name the rename would actually produce. The facts above are what a refusal
    // names; this is the projection itself, so a change in how those facts compose
    // into a basename cannot pass while each fact individually matches.
    match projected_destination(source, document, package) {
        Some(destination) => fingerprint.observed("destination", path_bytes(&destination)),
        None => fingerprint.absent("destination"),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderAlignmentResult {
    order: HealthyOrder,
    index: PackageIndexRecord,
}

impl OrderAlignmentResult {
    /// The same Order, at its new path.
    pub fn order(&self) -> &HealthyOrder {
        &self.order
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderAlignmentRejection<E> {
    /// The source or destination changed between planning and commit.
    ///
    /// **A `Location` rather than a `PathBuf`**, for the reason
    /// [`OrderAlignmentPlanError::DestinationCollision`] carries one: this refusal happens
    /// under a Root the plan established, about a path the plan itself derived inside it,
    /// and a response owes that Root's spelling for what it names — in a refusal exactly as
    /// in a success. Handing back a bare path is what left an adapter to re-derive the
    /// spelling or drop it.
    PreconditionsChanged {
        location: Location,
    },
    /// The facts this alignment was authorized against no longer stand, so the plan no
    /// longer names the right Order or the right name to give it. The seam's own answer
    /// is carried rather than re-encoded, so a caller can say which fact moved; it is
    /// only ever constructed from an outcome that was not [`EvidenceOutcome::Unchanged`].
    PlanEvidenceUnconfirmed {
        outcome: EvidenceOutcome,
    },
    /// The projected name was taken before the rename ran.
    DestinationCollision {
        location: Location,
    },
    RenameFailed {
        location: Location,
        error: E,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderAlignmentWarning {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
}

impl OrderAlignmentWarning {
    /// This warning's one stable spelling.
    ///
    /// Beside the warning rather than in an adapter, and deliberately the same two words
    /// `crate::creation` and `crate::adoption` use: a committed mutation whose derived
    /// refresh failed is one condition, so a consumer branching on it reads one spelling
    /// however the Package came to need refreshing.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::IndexRefreshFailed(_) => "index_refresh_failed",
            Self::RefreshUnavailable => "refresh_unavailable",
        }
    }
}

pub type OrderAlignmentReport<E> = MutationReport<
    OrderAlignmentPlan,
    OrderAlignmentResult,
    OrderAlignmentRejection<E>,
    OrderAlignmentWarning,
>;

pub fn commit_order_alignment<F: FileSystem + DirectoryFileSystem>(
    plan: OrderAlignmentPlan,
    filesystem: &F,
) -> OrderAlignmentReport<F::Error> {
    match filesystem.symlink_metadata(plan.source.absolute_path()) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        // A source that vanished or is no longer an ordinary directory is not the
        // Order this plan validated. Checked here rather than left to the resolution
        // below, which canonicalizes: a link standing where the directory was would
        // resolve to whatever it points at and then be renamed in its place.
        _ => {
            let source = plan.source.clone();
            return preconditions_changed(plan, source);
        }
    }

    // Re-resolve the subject rather than settle for having found something there. A
    // source that no longer resolves to a healthy Order of its own Package is neither
    // this plan's Order nor identifiable as any other.
    let Ok((package, order)) = resolve_order(plan.source.absolute_path(), filesystem) else {
        let source = plan.source.clone();
        return preconditions_changed(plan, source);
    };

    // Immediately before the first effect, which is the rename below or — for an
    // already aligned Order — the report's standing claim that this Order's facts
    // project the name it already carries.
    let outcome = plan.evidence.confirm(&alignment_evidence(&package, &order));
    if !outcome.is_unchanged() {
        return MutationReport::Rejected {
            plan,
            rejection: OrderAlignmentRejection::PlanEvidenceUnconfirmed { outcome },
        };
    }

    // An already aligned Order is committed by performing nothing.
    if !plan.changes_directory_name() {
        let result = OrderAlignmentResult {
            order,
            index: package.index().clone(),
        };
        return MutationReport::Unchanged { plan, result };
    }

    // The whole directory moves as one opaque tree. Nothing enumerates, copies, or
    // rewrites its children, so README and Supporting Material bytes travel intact.
    match filesystem.rename_new_directory(
        plan.source.absolute_path(),
        plan.destination.absolute_path(),
    ) {
        Ok(NewDirectoryRenameOutcome::Renamed) => {}
        Ok(NewDirectoryRenameOutcome::DestinationExists) => {
            let location = plan.destination.clone();
            return MutationReport::Rejected {
                plan,
                rejection: OrderAlignmentRejection::DestinationCollision { location },
            };
        }
        Err(error) => {
            let location = plan.destination.clone();
            return MutationReport::Rejected {
                plan,
                rejection: OrderAlignmentRejection::RenameFailed { location, error },
            };
        }
    }

    match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => match order_at(&package, &plan) {
            Some(order) => {
                let result = OrderAlignmentResult {
                    order,
                    index: package.index().clone(),
                };
                MutationReport::Committed { plan, result }
            }
            // The rename already happened, so this is never a rejection. The Order
            // is no longer discoverable, which the caller learns as a warning over
            // its own projection rather than as work that did not occur.
            None => {
                let index = package.index().clone();
                committed_with_warning(plan, index, OrderAlignmentWarning::RefreshUnavailable)
            }
        },
        PackageLoad::IndexReplacementFailed(failed) => {
            let warning = OrderAlignmentWarning::IndexRefreshFailed(failed.failure().clone());
            let index = failed.canonical_evidence().index().clone();
            match order_at(failed.canonical_evidence(), &plan) {
                Some(order) => {
                    let result = OrderAlignmentResult { order, index };
                    MutationReport::CommittedWithWarning {
                        plan,
                        result,
                        warnings: vec![warning],
                    }
                }
                None => committed_with_warning(plan, index, warning),
            }
        }
        PackageLoad::OperationFailed(failed) => {
            let warning = OrderAlignmentWarning::IndexRefreshFailed(failed.failure().clone());
            let index = plan_index_after_rename(&plan);
            committed_with_warning(plan, index, warning)
        }
        _ => {
            let index = plan_index_after_rename(&plan);
            committed_with_warning(plan, index, OrderAlignmentWarning::RefreshUnavailable)
        }
    }
}

/// The derived index as it stands after a rename no refresh could confirm.
///
/// A rename moves the Order to a new directory name inside the same Orders
/// Directory, so the indexed set of Order paths no longer matches the discovered
/// set — which is what `OrderMembershipChanged` names. It is deliberately not
/// `OrderPathMismatch`: that reason means an index entry points somewhere it
/// structurally may not, outside the Orders Directory, and a rename produces a
/// perfectly valid direct child. It is deliberately not `OrderMetadataChanged`
/// either, because an alignment writes no Order record. This is the same reason the
/// detector reaches on its own when it can inspect the renamed Package, so a caller
/// comparing this record against a later real refresh sees one consistent answer.
fn plan_index_after_rename(plan: &OrderAlignmentPlan) -> PackageIndexRecord {
    plan.prior_index
        .after_committed_membership_change_without_refresh()
}

/// The rename already succeeded, so the report stays committed and carries the Order
/// as this alignment left it; retrying the rename is not what a caller should do.
fn committed_with_warning<E>(
    plan: OrderAlignmentPlan,
    index: PackageIndexRecord,
    warning: OrderAlignmentWarning,
) -> OrderAlignmentReport<E> {
    let order = crate::canonical::project_committed_order(
        &plan.root,
        plan.destination.absolute_path(),
        &plan.document,
        plan.readme_text.clone(),
    );
    let result = OrderAlignmentResult { order, index };
    MutationReport::CommittedWithWarning {
        plan,
        result,
        warnings: vec![warning],
    }
}

fn order_at(package: &CurrentPackageCandidate, plan: &OrderAlignmentPlan) -> Option<HealthyOrder> {
    package
        .healthy_orders()
        .iter()
        .find(|candidate| candidate.document().order_id == plan.document.order_id)
        .cloned()
}

/// One planned path, as this Package locates it.
///
/// The Package-relative spelling holds by construction, as it does for a planned creation
/// artifact: the source is a discovered Order Directory, whose own `Location` the load
/// already carries, and the destination is that directory's parent joined with a
/// projected basename `normalize_filename_component` has already stripped of separators.
fn located(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned alignment path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned alignment path is normalized")
}

fn preconditions_changed<E>(
    plan: OrderAlignmentPlan,
    location: Location,
) -> OrderAlignmentReport<E> {
    MutationReport::Rejected {
        plan,
        rejection: OrderAlignmentRejection::PreconditionsChanged { location },
    }
}
