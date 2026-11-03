//! Applying exactly one explicitly decided semantic Order repair.
//!
//! The resolutions here are bounded and were each decided in advance: an unresolved
//! Vendor Reference may be pointed at an existing Vendor, replaced by a deliberately
//! created distinct Vendor, or cleared; one side of a duplicate Order ID conflict may
//! adopt a fresh identity; and a stale technical locator may be recomputed from the
//! physical Package relationship. Nothing here infers a fact from a path, a Contact,
//! another Order, or Supporting Material, and Vendor Merge does not exist.

use crate::{
    canonical::{
        CanonicalPackageLoad, PackageIndexRecord, PackageLoad, inspect_canonical_package,
        open_package,
    },
    filesystem::{
        FileReplacement, MultiFileReplacementError, RefusedRollback, RollbackFailure,
        replace_validated_files,
    },
    metadata::{OrderDocument, Vendor, VendorsDocument},
    mutation::MutationReport,
    operations::OperationFailure,
    order_diagnosis::{OrderDiagnosis, OrderDiagnosisError, diagnose_order},
    package::vendor_aggregate_is_package_valid,
    ports::{FileSystem, UuidGenerator},
    readme::{Location, RootedLocation, located_in_package, rooted_at},
    repair_cases::RepairAction,
    vendors::{vendor_by_name, vendor_names_match},
};
use std::path::{Path, PathBuf};

/// The explicit resolution a caller chose. Each names everything the repair needs;
/// none of them is inferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticResolution {
    /// Point the unresolved reference at the existing Vendor this Vendor Name selects.
    ///
    /// The Name is the ordinary Vendor selector — trimmed, matched without regard to
    /// letter case, unique within the Package, and resolving a Hidden Vendor like any
    /// other — and the repaired record stores the Vendor ID it resolves to.
    AssignExistingVendor { name: String },
    /// Deliberately create one distinct Vendor with this Name and assign it. This is
    /// a new Vendor, never a merge with or a redirection to an existing one.
    CreateAndAssignVendor { name: String },
    /// Drop the Vendor Reference outright.
    ClearVendorReference,
    /// Adopt a fresh identity for this Order, which the caller has explicitly chosen
    /// as the non-surviving side of the conflict.
    AdoptNewOrderIdentity,
    /// Recompute the technical locator from the physical Package relationship.
    RepairPackageLocator,
}

impl SemanticResolution {
    /// The diagnosed action this resolution carries out.
    pub fn action(&self) -> RepairAction {
        match self {
            Self::AssignExistingVendor { .. } => RepairAction::AssignExistingVendor,
            Self::CreateAndAssignVendor { .. } => RepairAction::CreateAndAssignVendor,
            Self::ClearVendorReference => RepairAction::ClearVendorReference,
            Self::AdoptNewOrderIdentity => RepairAction::AdoptNewOrderIdentity,
            Self::RepairPackageLocator => RepairAction::RepairPackageLocator,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticRepairOperationKind {
    ReplaceOrderRecord,
    ReplaceVendorRecord,
    RefreshPackageIndex,
}

impl SemanticRepairOperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReplaceOrderRecord => "replace_order_record",
            Self::ReplaceVendorRecord => "replace_vendor_record",
            Self::RefreshPackageIndex => "refresh_package_index",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticRepairOperation {
    kind: SemanticRepairOperationKind,
    location: Location,
}

impl SemanticRepairOperation {
    pub fn kind(&self) -> SemanticRepairOperationKind {
        self.kind
    }

    /// Where this operation writes, as the Package that holds it names it — so a caller
    /// reporting it carries the Package-relative spelling as well as the absolute path.
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn path(&self) -> &Path {
        self.location.absolute_path()
    }
}

/// Where a newly created Vendor's identity comes from. Like a created Order's, it is
/// generated only at commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlannedVendorIdentity {
    None,
    GeneratedAtCommit { name: String },
}

/// One complete, validated semantic repair of one diagnosed case.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticRepairPlan {
    directory: PathBuf,
    root: PathBuf,
    handle: String,
    resolution: SemanticResolution,
    order_path: PathBuf,
    vendors_path: PathBuf,
    prior_order: OrderDocument,
    prior_vendors: VendorsDocument,
    repaired_order: OrderDocument,
    created_vendor: PlannedVendorIdentity,
    generates_order_id: bool,
    operations: Vec<SemanticRepairOperation>,
}

impl SemanticRepairPlan {
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The SpielOrders Root the Order scope belongs to.
    ///
    /// Planning resolved it, so every refusal raised from this plan onward has a Root to
    /// name — including the two a minted identity reaches, whose own values carry none.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn resolution(&self) -> &SemanticResolution {
        &self.resolution
    }

    /// The complete Order this repair would leave behind. A generated identity is
    /// still absent here, because it is created only at commit.
    pub fn repaired_order(&self) -> &OrderDocument {
        &self.repaired_order
    }

    pub fn created_vendor(&self) -> &PlannedVendorIdentity {
        &self.created_vendor
    }

    pub fn operations(&self) -> &[SemanticRepairOperation] {
        &self.operations
    }
}

/// Why no semantic repair could be planned.
///
/// **Every refusal below the diagnosis carries a [`RootedLocation`]**, because every one of
/// them is raised after [`diagnose_order`] established the Package and the Order Directory
/// inside it: the scope was resolved, so the response owes both the Root it was resolved
/// under and the Package's own spelling of whatever the refusal is about. Only
/// [`Self::Diagnosis`] is raised before there is a Package to name.
#[derive(Debug, PartialEq)]
pub enum SemanticRepairError<E> {
    Diagnosis(OrderDiagnosisError<E>),
    /// No currently diagnosed case answers to this handle.
    UnknownCase {
        handle: String,
        scope: RootedLocation,
    },
    /// The named case does not authorize this resolution.
    UnauthorizedAction {
        handle: String,
        action: RepairAction,
        scope: RootedLocation,
    },
    /// No Vendor of this Package carries the chosen Name.
    UnknownVendor {
        name: String,
        scope: RootedLocation,
    },
    /// A deliberately created Vendor must carry a Name no existing Vendor already
    /// uses; this core never merges two Vendors or redirects one to another.
    VendorNameCollision {
        name: String,
        scope: RootedLocation,
    },
    /// A created Vendor needs a real Name. A placeholder is not a Vendor.
    MissingVendorName {
        scope: RootedLocation,
    },
    InvalidRepairedOrder {
        scope: RootedLocation,
    },
    FileSystem {
        subject: RootedLocation,
        error: E,
    },
}

/// The one refusal the record-shaping helpers can reach, before the scope that reports it
/// is attached.
///
/// They rebuild an Order record from its own fields, so the only way any of them fails is
/// that the record this resolution would leave behind is not a valid Order. Carrying the
/// scope through each of them instead would thread it through four helpers to say one
/// thing.
struct UnshapableOrder;

/// Plans exactly one explicit semantic resolution against current evidence.
pub fn plan_semantic_repair<F: FileSystem>(
    order_directory: &Path,
    handle: &str,
    resolution: &SemanticResolution,
    filesystem: &F,
) -> Result<SemanticRepairPlan, SemanticRepairError<F::Error>> {
    let diagnosis =
        diagnose_order(order_directory, filesystem).map_err(SemanticRepairError::Diagnosis)?;
    let directory = diagnosis.directory().to_path_buf();
    // The Package the diagnosis was taken against, never one inferred by counting
    // parents: the Orders Directory is configured and may sit at any depth, so two
    // levels up from an Order Directory names an ordinary folder of the user's.
    let root = diagnosis.package_root().to_path_buf();
    let scope = || rooted_at(&root, directory.clone());
    let unshapable = |UnshapableOrder| SemanticRepairError::InvalidRepairedOrder { scope: scope() };
    let case = diagnosis
        .case(handle)
        .ok_or_else(|| SemanticRepairError::UnknownCase {
            handle: handle.to_owned(),
            scope: scope(),
        })?;
    if !case.actions().contains(&resolution.action()) {
        return Err(SemanticRepairError::UnauthorizedAction {
            handle: handle.to_owned(),
            action: resolution.action(),
            scope: scope(),
        });
    }

    let order_path = directory.join(".spielorders").join("order.json");
    let vendors_path = root.join(".spielorders").join("vendors.json");
    let prior_order = read_document::<OrderDocument, F>(&root, &order_path, filesystem)?;
    let prior_vendors = read_document::<VendorsDocument, F>(&root, &vendors_path, filesystem)?;

    let mut repaired = prior_order.clone();
    let mut created_vendor = PlannedVendorIdentity::None;
    let mut generates_order_id = false;
    let mut operations = vec![SemanticRepairOperation {
        kind: SemanticRepairOperationKind::ReplaceOrderRecord,
        location: located_in_package(&root, order_path.clone()),
    }];

    match resolution {
        SemanticResolution::AssignExistingVendor { name } => {
            // The chosen Vendor must already exist; nothing here creates one by
            // accident or resolves a reference from anything but this explicit choice.
            let Some(vendor) = vendor_by_name(name, &prior_vendors.vendors) else {
                return Err(SemanticRepairError::UnknownVendor {
                    name: name.clone(),
                    scope: scope(),
                });
            };
            set_vendor(&mut repaired, Some(vendor.vendor_id.as_str())).map_err(unshapable)?;
        }
        SemanticResolution::CreateAndAssignVendor { name } => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(SemanticRepairError::MissingVendorName { scope: scope() });
            }
            // A deliberately distinct Vendor, never a merge: a Name an existing
            // Vendor already uses would be that Vendor, so it is refused.
            if let Some(existing) = prior_vendors
                .vendors
                .iter()
                .find(|vendor| vendor_names_match(vendor.name.as_str(), trimmed))
            {
                return Err(SemanticRepairError::VendorNameCollision {
                    name: existing.name.as_str().to_owned(),
                    scope: scope(),
                });
            }
            created_vendor = PlannedVendorIdentity::GeneratedAtCommit {
                name: trimmed.to_owned(),
            };
            operations.push(SemanticRepairOperation {
                kind: SemanticRepairOperationKind::ReplaceVendorRecord,
                location: located_in_package(&root, vendors_path.clone()),
            });
        }
        SemanticResolution::ClearVendorReference => {
            set_vendor(&mut repaired, None).map_err(unshapable)?;
        }
        SemanticResolution::AdoptNewOrderIdentity => {
            generates_order_id = true;
        }
        SemanticResolution::RepairPackageLocator => {
            // Derived from where the directory physically sits, never from the stored
            // value, the directory name, or a guess. The diagnosis found this root by
            // walking the Order Directory's own ancestors for a Package marker, so the
            // directory is strictly inside it and the relative path it yields carries at
            // least one component — the same relationship `creation` relies on when it
            // computes a locator for a newly created Order.
            let depth = directory
                .strip_prefix(&root)
                .expect("the diagnosed Order Directory is inside its Package")
                .components()
                .count();
            set_locator(&mut repaired, &vec![".."; depth].join("/")).map_err(unshapable)?;
        }
    }

    operations.push(SemanticRepairOperation {
        kind: SemanticRepairOperationKind::RefreshPackageIndex,
        location: located_in_package(&root, root.join(".spielorders").join("index.json")),
    });

    Ok(SemanticRepairPlan {
        directory,
        root,
        handle: handle.to_owned(),
        resolution: resolution.clone(),
        order_path,
        vendors_path,
        prior_order,
        prior_vendors,
        repaired_order: repaired,
        created_vendor,
        generates_order_id,
        operations,
    })
}

fn read_document<T: serde::de::DeserializeOwned, F: FileSystem>(
    root: &Path,
    path: &Path,
    filesystem: &F,
) -> Result<T, SemanticRepairError<F::Error>> {
    read_document_bytes(root, path, filesystem).map(|(document, _)| document)
}

/// The parsed document and the exact bytes it was parsed from. The bytes are the
/// evidence this commit was authorized against, and they are what the replacement
/// refuses to write over if anyone else changes the file first.
fn read_document_bytes<T: serde::de::DeserializeOwned, F: FileSystem>(
    root: &Path,
    path: &Path,
    filesystem: &F,
) -> Result<(T, Vec<u8>), SemanticRepairError<F::Error>> {
    let subject = || rooted_at(root, path.to_path_buf());
    let bytes = filesystem
        .read(path)
        .map_err(|error| SemanticRepairError::FileSystem {
            subject: subject(),
            error,
        })?;
    let document = serde_json::from_slice(&bytes)
        .map_err(|_| SemanticRepairError::InvalidRepairedOrder { scope: subject() })?;
    Ok((document, bytes))
}

fn set_vendor(
    document: &mut OrderDocument,
    vendor_id: Option<&str>,
) -> Result<(), UnshapableOrder> {
    let mut object = serde_json::to_value(&*document)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(UnshapableOrder)?;
    match vendor_id {
        Some(vendor_id) => {
            object.insert(
                "vendor_id".into(),
                serde_json::Value::String(vendor_id.to_owned()),
            );
        }
        None => {
            object.remove("vendor_id");
        }
    }
    *document = rebuilt(object)?;
    Ok(())
}

fn set_locator(document: &mut OrderDocument, locator: &str) -> Result<(), UnshapableOrder> {
    let mut object = serde_json::to_value(&*document)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(UnshapableOrder)?;
    object.insert(
        "package_root".into(),
        serde_json::Value::String(locator.to_owned()),
    );
    *document = rebuilt(object)?;
    Ok(())
}

fn rebuilt(
    object: serde_json::Map<String, serde_json::Value>,
) -> Result<OrderDocument, UnshapableOrder> {
    serde_json::to_vec(&serde_json::Value::Object(object))
        .ok()
        .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
        .ok_or(UnshapableOrder)
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticRepairResult {
    order: OrderDocument,
    created_vendor: Option<Vendor>,
    diagnosis: Option<OrderDiagnosis>,
    index: Option<PackageIndexRecord>,
}

impl SemanticRepairResult {
    /// The complete repaired Order.
    pub fn order(&self) -> &OrderDocument {
        &self.order
    }

    /// The Vendor this repair deliberately created, when it created one.
    pub fn created_vendor(&self) -> Option<&Vendor> {
        self.created_vendor.as_ref()
    }

    /// Everything still diagnosed in this scope afterwards, including any conflict
    /// this single resolution deliberately left standing.
    pub fn remaining(&self) -> Option<&OrderDiagnosis> {
        self.diagnosis.as_ref()
    }

    pub fn index(&self) -> Option<&PackageIndexRecord> {
        self.index.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticCandidateFailure {
    InvalidSemantics,
}

/// Why a planned semantic repair was refused at the write, or failed there.
///
/// **Every one that is about a path carries a [`RootedLocation`]**, for the same reason
/// [`SemanticRepairError`] does: a rejection is raised with the plan in hand, so the Root
/// and the Package's own spelling of the path are both known and a response owes both.
#[derive(Debug, PartialEq)]
pub enum SemanticRepairRejection<E> {
    /// The conflict set or the record changed between planning and commit.
    EvidenceChanged { evidence: RootedLocation },
    /// A freshly generated identity already exists in this Package.
    GeneratedIdentityCollision { order_id: String },
    /// The Package could not be read as a current Package while a generated identity
    /// was being checked against it. Nothing shows the identity is free, so the repair
    /// is refused rather than written against evidence nobody has.
    ///
    /// Distinct from a collision on purpose. A collision is answered by generating
    /// another identity; this is not answered by any identity at all, and reporting it
    /// as one would send a caller to retry forever. `metadata_repair` refuses the same
    /// state under the same name.
    PackageEvidenceUnavailable { root: RootedLocation },
    /// A freshly generated Vendor identity already exists in this Package, so the
    /// aggregate this repair assembled is not a valid Package Vendor set.
    GeneratedVendorIdentityCollision { vendor_id: String },
    ReplacementFailed {
        /// Every managed file this repair wrote that did not go back to what it held,
        /// in the order the plan named them. **Empty is the ordinary case** and is what
        /// lets a refusal say every managed file still holds what it held; a non-empty
        /// one is the restoration failures and the restorations declined because
        /// someone else had since written the destination, which
        /// [`MultiFileReplacementError`] retains and which a response must name rather
        /// than summarize.
        unrestored: Vec<RootedLocation>,
        error: MultiFileReplacementError<E, SemanticCandidateFailure>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticRepairWarning {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
    RediagnosisUnavailable,
}

pub type SemanticRepairReport<E> = MutationReport<
    SemanticRepairPlan,
    SemanticRepairResult,
    SemanticRepairRejection<E>,
    SemanticRepairWarning,
>;

pub fn commit_semantic_repair<F: FileSystem>(
    plan: SemanticRepairPlan,
    uuids: &mut impl UuidGenerator,
    filesystem: &F,
) -> SemanticRepairReport<F::Error> {
    // The evidence this plan was built from must still stand.
    let Ok((current_order, observed_order)) =
        read_document_bytes::<OrderDocument, F>(&plan.root, &plan.order_path, filesystem)
    else {
        let path = plan.order_path.clone();
        return evidence_changed(plan, path);
    };
    if current_order != plan.prior_order {
        let path = plan.order_path.clone();
        return evidence_changed(plan, path);
    }
    let Ok((current_vendors, observed_vendors)) =
        read_document_bytes::<VendorsDocument, F>(&plan.root, &plan.vendors_path, filesystem)
    else {
        let path = plan.vendors_path.clone();
        return evidence_changed(plan, path);
    };
    if current_vendors != plan.prior_vendors {
        let path = plan.vendors_path.clone();
        return evidence_changed(plan, path);
    }

    let mut repaired = plan.repaired_order.clone();
    let mut created = None;
    if plan.generates_order_id {
        let order_id = uuids.generate_v4();
        // A fresh identity is checked against the Package before it is written, so a
        // repair never resolves one conflict by creating another. Both refusals below
        // keep that guarantee; they differ in what the caller can do about it.
        match generated_identity_verdict(&plan, &order_id, filesystem) {
            GeneratedIdentityVerdict::Free => {}
            GeneratedIdentityVerdict::Taken => {
                return MutationReport::Rejected {
                    plan,
                    rejection: SemanticRepairRejection::GeneratedIdentityCollision { order_id },
                };
            }
            GeneratedIdentityVerdict::PackageEvidenceUnavailable => {
                let root = rooted_at(&plan.root, plan.root.clone());
                return MutationReport::Rejected {
                    plan,
                    rejection: SemanticRepairRejection::PackageEvidenceUnavailable { root },
                };
            }
        }
        if set_identity(&mut repaired, &order_id).is_err() {
            let path = plan.order_path.clone();
            return evidence_changed(plan, path);
        }
    }
    let mut vendors = plan.prior_vendors.clone();
    if let PlannedVendorIdentity::GeneratedAtCommit { name } = &plan.created_vendor {
        let vendor_id = uuids.generate_v4();
        let Ok(vendor) = serde_json::from_value::<Vendor>(serde_json::json!({
            "vendor_id": vendor_id,
            "name": name,
            "contacts": [],
        })) else {
            let path = plan.vendors_path.clone();
            return evidence_changed(plan, path);
        };
        if set_vendor(&mut repaired, Some(vendor.vendor_id.as_str())).is_err() {
            let path = plan.order_path.clone();
            return evidence_changed(plan, path);
        }
        vendors.vendors.push(vendor.clone());
        // A created Vendor joins the whole Vendor aggregate, and an aggregate is a
        // Package fact: duplicate Vendor IDs and package-duplicate Vendor Names are
        // cross-record constraints that deserializing the candidate cannot see. The
        // aggregate this repair would leave behind is judged by the same Package
        // semantics canonical validation applies, before either managed record is
        // written, so a repair can never answer one diagnosed condition by damaging
        // the Package's Vendor set.
        //
        // The Name was already refused while planning, under the same case folding
        // that validation uses, and the aggregate has not changed since — the commit
        // revalidation above proves it. The identity is the one fact here the plan
        // could not judge, because it is generated at commit, so that is what a
        // failure here reports.
        //
        // The caller's own aggregate is what goes on to be written, not a normalized
        // copy: a created Vendor carries a Name already trimmed while planning and no
        // other field, which is all Vendor normalization touches.
        if !vendor_aggregate_is_package_valid(&vendors) {
            let vendor_id = vendor.vendor_id.as_str().to_owned();
            return MutationReport::Rejected {
                plan,
                rejection: SemanticRepairRejection::GeneratedVendorIdentityCollision { vendor_id },
            };
        }
        created = Some(vendor);
    }

    let Ok(order_bytes) = repaired.to_json_bytes() else {
        let path = plan.order_path.clone();
        return evidence_changed(plan, path);
    };
    let Ok(vendor_bytes) = vendors.to_json_bytes() else {
        let path = plan.vendors_path.clone();
        return evidence_changed(plan, path);
    };

    // One prevalidated plan. A Vendor creation and the Order reassignment that needs
    // it are written together or not at all, with every restoration failure retained.
    // Each replacement carries the exact bytes this commit revalidated above, so a
    // writer landing between that revalidation and the write is refused rather than
    // overwritten.
    let mut replacements = vec![FileReplacement::observed(
        &plan.order_path,
        &order_bytes,
        &observed_order,
    )];
    if created.is_some() {
        replacements.push(FileReplacement::observed(
            &plan.vendors_path,
            &vendor_bytes,
            &observed_vendors,
        ));
    }
    if let Err(error) =
        replace_validated_files(&replacements, filesystem, |destination, candidate| {
            if destination == plan.order_path {
                // This one can fire, and it is the only thing on this path that
                // validates. A semantic repair reads its subject with
                // `serde_json::from_slice` alone and rewrites only a Vendor Reference,
                // the physical locator, or the Order identity — so a Damaged Order's
                // defect is carried through untouched. Quarantine does not exclude it:
                // an Order quarantined for an unreal `date_ordered` keeps its Order id,
                // which is what admits it to duplicate detection and to the
                // `AdoptNewOrderIdentity` repair. Pinned end to end by
                // `a_damaged_order_with_an_unreal_date_is_refused_at_the_candidate_guard`.
                OrderDocument::from_json_bytes(candidate)
                    .ok()
                    // The locator carries its own outcome rather than failing the
                    // record, so a candidate that would leave one this build cannot
                    // follow is refused here: a repair writes a record valid under the
                    // version it declares, and the locator's own repair is what
                    // corrects that damage.
                    .filter(|document| document.package_root.spelled().is_some())
                    .and_then(|document| document.validate().ok())
                    .map(|_| ())
                    .ok_or(SemanticCandidateFailure::InvalidSemantics)
            } else if destination == plan.vendors_path {
                // The Vendors candidate gets its own verdict rather than surviving
                // next to the arm above, and that verdict was already reached: these
                // bytes are `vendors.to_json_bytes()` of the aggregate
                // `vendor_aggregate_is_package_valid` accepted above, under the same
                // Package semantics canonical validation applies. Parsing them again
                // here would answer a smaller question than the one already answered,
                // and would only fail on a serializer bug — unreachable by
                // construction, for the reason that retired the guard in `vendors.rs`.
                Ok(())
            } else {
                // A repair writes the two destinations named above and no others, so
                // an unnamed one is not a candidate this commit assembled. It is
                // refused rather than admitted by an arm that only meant "not the
                // Order record".
                Err(SemanticCandidateFailure::InvalidSemantics)
            }
        })
    {
        let unrestored = unrestored_destinations(&plan.root, &error);
        return MutationReport::Rejected {
            plan,
            rejection: SemanticRepairRejection::ReplacementFailed { unrestored, error },
        };
    }

    let mut warnings = Vec::new();
    let index = match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => Some(package.index().clone()),
        PackageLoad::IndexReplacementFailed(failed) => {
            warnings.push(SemanticRepairWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            Some(failed.canonical_evidence().index().clone())
        }
        PackageLoad::OperationFailed(failed) => {
            warnings.push(SemanticRepairWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            None
        }
        _ => {
            warnings.push(SemanticRepairWarning::RefreshUnavailable);
            None
        }
    };
    let diagnosis = match diagnose_order(&plan.directory, filesystem) {
        Ok(diagnosis) => Some(diagnosis),
        Err(_) => {
            warnings.push(SemanticRepairWarning::RediagnosisUnavailable);
            None
        }
    };

    let result = SemanticRepairResult {
        order: repaired,
        created_vendor: created,
        diagnosis,
        index,
    };
    if warnings.is_empty() {
        MutationReport::Committed { plan, result }
    } else {
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        }
    }
}

/// What this Package can say about a freshly generated Order identity.
///
/// Three answers, because there are three: the Package holds the identity, the Package
/// does not, and the Package could not be read. Collapsing the third into either of the
/// first two is the whole hazard — into `Taken` it becomes a collision a caller retries
/// against forever, and into `Free` it writes an identity nothing showed was free.
enum GeneratedIdentityVerdict {
    Free,
    Taken,
    PackageEvidenceUnavailable,
}

/// What this Package says about a generated identity, so a repair never lands one on an
/// existing conflict and never writes one nothing showed was free.
///
/// Asked of `inspect_canonical_package`, which reads. `open_package` rebuilds, and a
/// rebuild durably replaces `.spielorders/index.json` — so answering through it would
/// let a repair that goes on to refuse have already rewritten a canonical artifact.
/// Every commit precondition in this crate that consults a Package reads it.
fn generated_identity_verdict<F: FileSystem>(
    plan: &SemanticRepairPlan,
    order_id: &str,
    filesystem: &F,
) -> GeneratedIdentityVerdict {
    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(&plan.root, filesystem)
    else {
        // Unmanaged, damaged, upgrade-pending, and outright failure alike: without a
        // trustworthy view of the Package an identity cannot be shown to be free, so it
        // is refused rather than assumed safe. What separates them is why the Package
        // cannot be read, which is a diagnosis rather than a repair precondition, and
        // the caller reaches it through `diagnose_order` as it did to plan this repair.
        return GeneratedIdentityVerdict::PackageEvidenceUnavailable;
    };
    // Every discovered Order counts, not only the healthy ones. Orders sharing an
    // identity are quarantined as damaged precisely because of that conflict, so
    // consulting the healthy set alone would declare the contested identity free.
    let taken = package
        .healthy_orders()
        .iter()
        .any(|order| order.document().order_id.as_str() == order_id)
        || package.damaged_orders().iter().any(|order| {
            order
                .order_id()
                .is_some_and(|existing| existing.as_str() == order_id)
        });
    if taken {
        GeneratedIdentityVerdict::Taken
    } else {
        GeneratedIdentityVerdict::Free
    }
}

fn set_identity(document: &mut OrderDocument, order_id: &str) -> Result<(), UnshapableOrder> {
    let mut object = serde_json::to_value(&*document)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(UnshapableOrder)?;
    object.insert(
        "order_id".into(),
        serde_json::Value::String(order_id.to_owned()),
    );
    *document = rebuilt(object)?;
    Ok(())
}

/// Every destination a failed multi-file replacement did not put back.
///
/// Two ways a file can be left holding something other than what it held: a restoration
/// that was attempted and failed, and one this core declined because the destination had
/// since been written by someone else. Both are the same fact to a reader — this file is
/// not as it was — so both are named, and a refusal that reports neither is entitled to
/// say every managed file holds what it held.
fn unrestored_destinations<E, V>(
    root: &Path,
    error: &MultiFileReplacementError<E, V>,
) -> Vec<RootedLocation> {
    error
        .rollback_failures()
        .iter()
        .map(RollbackFailure::destination)
        .chain(
            error
                .refused_rollbacks()
                .iter()
                .map(RefusedRollback::destination),
        )
        .map(|destination| rooted_at(root, destination.to_path_buf()))
        .collect()
}

fn evidence_changed<E>(plan: SemanticRepairPlan, path: PathBuf) -> SemanticRepairReport<E> {
    let evidence = rooted_at(&plan.root, path);
    MutationReport::Rejected {
        plan,
        rejection: SemanticRepairRejection::EvidenceChanged { evidence },
    }
}
