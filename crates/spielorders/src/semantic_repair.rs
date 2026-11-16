//! Applying exactly one explicitly decided semantic Order repair.
//!
//! The resolutions here are bounded and each one decided: an unresolved
//! Vendor Reference may be pointed at an existing Vendor, replaced by a deliberately
//! created distinct Vendor, or cleared; one side of a duplicate Order ID conflict may
//! adopt a fresh identity; and a stale technical locator may be recomputed from the
//! physical Package relationship. Nothing here infers a fact from a path, a Contact,
//! another Order, or Supporting Material, and Vendor Merge does not exist.

use crate::{
    Failure,
    canonical::load_every_record,
    filesystem::{FileReplacement, replace_validated_files},
    metadata::{OrderDocument, Vendor, VendorsDocument},
    mutation::{OperationKind, PlannedOperation},
    operations::FailedOperation,
    package::vendor_aggregate_is_package_valid,
    ports::FileSystem,
    readme::{Location, located_in_package, rooted_at},
    vendors::{vendor_by_name, vendor_names_match},
};
use std::path::{Path, PathBuf};

/// One decided semantic resolution, as `repair` routes it here from the caller's
/// resolution.
pub(crate) enum SemanticResolution<'a> {
    AssignExistingVendor(&'a str),
    CreateAndAssignVendor(&'a str),
    ClearVendorReference,
    AdoptNewOrderIdentity,
    RepairPackageLocator,
}

/// One complete, validated semantic repair of one diagnosed case.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticRepairPlan {
    directory: Location,
    root: PathBuf,
    order_path: PathBuf,
    vendors_path: PathBuf,
    prior_order: OrderDocument,
    prior_vendors: VendorsDocument,
    repaired_order: OrderDocument,
    created_vendor: Option<String>,
    generates_order_id: bool,
    pub(crate) operations: Vec<PlannedOperation>,
}

impl SemanticRepairPlan {
    pub fn directory(&self) -> &Path {
        &self.directory.absolute_path
    }

    /// The SpielOrders Root the Order scope belongs to.
    ///
    /// Planning resolved it, so every refusal raised from this plan onward has a Root to
    /// name — including the two a minted identity reaches, whose own values carry none.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// The one refusal the record-shaping helpers can reach, before the scope that reports it
/// is attached.
///
/// They rebuild an Order record from its own fields, so the only way any of them fails is
/// that the record this resolution would leave behind is not a valid Order. Carrying the
/// scope through each of them instead would thread it through four helpers to say one
/// thing.
struct UnshapableOrder;

/// Plans exactly one explicit semantic resolution of a case its scope's diagnosis
/// authorizes, against current evidence.
///
/// `root` is the Package the diagnosis was taken against, never one inferred by counting
/// parents: the Orders Directory is configured and may sit at any depth, so two levels up
/// from an Order Directory names an ordinary folder of the user's.
pub(crate) fn plan<F: FileSystem>(
    root: &Path,
    directory: &Path,
    resolution: SemanticResolution<'_>,
    filesystem: &F,
) -> Result<SemanticRepairPlan, Failure> {
    let (root, directory) = (root.to_path_buf(), directory.to_path_buf());
    let scope = || rooted_at(&root, directory.clone());
    let unshapable = |UnshapableOrder| Failure::InvalidCandidate { candidate: scope() };

    let order_path = directory.join(".spielorders").join("order.json");
    let vendors_path = root.join(".spielorders").join("vendors.json");
    let prior_order = read_document::<OrderDocument, F>(&root, &order_path, filesystem)?;
    let prior_vendors = read_document::<VendorsDocument, F>(&root, &vendors_path, filesystem)?;

    let mut repaired = prior_order.clone();
    let mut created_vendor = None;
    let mut generates_order_id = false;
    let mut operations = vec![PlannedOperation::new(
        OperationKind::ReplaceOrderRecord,
        located_in_package(&root, order_path.clone()),
    )];

    match resolution {
        SemanticResolution::AssignExistingVendor(name) => {
            // The chosen Vendor must already exist; nothing here creates one by
            // accident or resolves a reference from anything but this explicit choice.
            let Some(vendor) = vendor_by_name(name, &prior_vendors.vendors) else {
                return Err(Failure::UnknownVendor {
                    name: name.to_owned(),
                    scope: rooted_at(&root, vendors_path.clone()),
                });
            };
            set_vendor(&mut repaired, Some(vendor.vendor_id.as_str())).map_err(unshapable)?;
        }
        SemanticResolution::CreateAndAssignVendor(name) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(Failure::MissingRequiredName {
                    scope: rooted_at(&root, vendors_path.clone()),
                });
            }
            // A deliberately distinct Vendor, never a merge: a Name an existing
            // Vendor already uses would be that Vendor, so it is refused.
            if let Some(existing) = prior_vendors
                .vendors
                .iter()
                .find(|vendor| vendor_names_match(vendor.name.as_str(), trimmed))
            {
                return Err(Failure::VendorNameInUse {
                    name: existing.name.as_str().to_owned(),
                    vendor_id: existing.vendor_id.as_str().to_owned(),
                    scope: rooted_at(&root, vendors_path.clone()),
                });
            }
            created_vendor = Some(trimmed.to_owned());
            operations.push(PlannedOperation::new(
                OperationKind::ReplaceVendorRecord,
                located_in_package(&root, vendors_path.clone()),
            ));
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

    operations.push(PlannedOperation::new(
        OperationKind::RefreshPackageIndex,
        located_in_package(&root, root.join(".spielorders").join("index.json")),
    ));

    Ok(SemanticRepairPlan {
        directory: located_in_package(&root, directory),
        root,
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
) -> Result<T, Failure> {
    read_document_bytes(root, path, filesystem).map(|(document, _)| document)
}

/// The parsed document and the exact bytes it was parsed from. The bytes are the
/// evidence this commit was authorized against, and they are what the replacement
/// refuses to write over if anyone else changes the file first.
fn read_document_bytes<T: serde::de::DeserializeOwned, F: FileSystem>(
    root: &Path,
    path: &Path,
    filesystem: &F,
) -> Result<(T, Vec<u8>), Failure> {
    let subject = || rooted_at(root, path.to_path_buf());
    let bytes = filesystem.read(path).map_err(|error| {
        Failure::from(FailedOperation::inside(
            root,
            path.to_path_buf(),
            &error,
            filesystem,
        ))
    })?;
    let document = serde_json::from_slice(&bytes).map_err(|_| Failure::InvalidCandidate {
        candidate: subject(),
    })?;
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

/// Writes one planned semantic repair, refusing it where the evidence it was planned
/// against no longer stands.
pub(crate) fn write<F: FileSystem>(
    plan: &SemanticRepairPlan,
    new_uuid: &dyn Fn() -> String,
    filesystem: &F,
) -> Result<(), Failure> {
    // The evidence this plan was built from must still stand.
    let Ok((current_order, observed_order)) =
        read_document_bytes::<OrderDocument, F>(&plan.root, &plan.order_path, filesystem)
    else {
        return Err(evidence_changed(plan, &plan.order_path));
    };
    if current_order != plan.prior_order {
        return Err(evidence_changed(plan, &plan.order_path));
    }
    let Ok((current_vendors, observed_vendors)) =
        read_document_bytes::<VendorsDocument, F>(&plan.root, &plan.vendors_path, filesystem)
    else {
        return Err(evidence_changed(plan, &plan.vendors_path));
    };
    if current_vendors != plan.prior_vendors {
        return Err(evidence_changed(plan, &plan.vendors_path));
    }

    let mut repaired = plan.repaired_order.clone();
    let mut creates_vendor = false;
    if plan.generates_order_id {
        let order_id = new_uuid();
        // A fresh identity is checked against the Package before it is written, so a
        // repair never resolves one conflict by creating another. Both refusals below
        // keep that guarantee; they differ in what the caller can do about it.
        match generated_identity_verdict(plan, &order_id, filesystem) {
            GeneratedIdentityVerdict::Free => {}
            GeneratedIdentityVerdict::Taken => {
                return Err(Failure::GeneratedIdentityCollision { order_id });
            }
            GeneratedIdentityVerdict::PackageUnusable(unusable) => {
                return Err(unusable);
            }
        }
        if set_identity(&mut repaired, &order_id).is_err() {
            return Err(evidence_changed(plan, &plan.order_path));
        }
    }
    let mut vendors = plan.prior_vendors.clone();
    if let Some(name) = &plan.created_vendor {
        let vendor_id = new_uuid();
        let Ok(vendor) = serde_json::from_value::<Vendor>(serde_json::json!({
            "vendor_id": vendor_id,
            "name": name,
            "contacts": [],
        })) else {
            return Err(evidence_changed(plan, &plan.vendors_path));
        };
        if set_vendor(&mut repaired, Some(vendor.vendor_id.as_str())).is_err() {
            return Err(evidence_changed(plan, &plan.order_path));
        }
        vendors.vendors.push(vendor);
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
            return Err(Failure::GeneratedVendorIdentityCollision { vendor_id });
        }
        creates_vendor = true;
    }

    let Ok(order_bytes) = repaired.to_json_bytes() else {
        return Err(evidence_changed(plan, &plan.order_path));
    };
    let Ok(vendor_bytes) = vendors.to_json_bytes() else {
        return Err(evidence_changed(plan, &plan.vendors_path));
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
    if creates_vendor {
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
                    .ok_or(())
            } else if destination == plan.vendors_path {
                // The Vendors candidate gets its own verdict rather than surviving
                // next to the arm above, and that verdict was already reached: these
                // bytes are `vendors.to_json_bytes()` of the aggregate
                // `vendor_aggregate_is_package_valid` accepted above, under the same
                // Package semantics canonical validation applies. Parsing them again
                // here would answer a smaller question than the one already answered,
                // and could only fail on a serializer bug.
                Ok(())
            } else {
                // A repair writes the two destinations named above and no others, so
                // an unnamed one is not a candidate this commit assembled. It is
                // refused rather than admitted by an arm that only meant "not the
                // Order record".
                Err(())
            }
        })
    {
        let destinations: Vec<&Path> = replacements
            .iter()
            .map(FileReplacement::destination)
            .collect();
        return Err(Failure::of_replacements(&plan.root, &destinations, error));
    }
    Ok(())
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
    PackageUnusable(Failure),
}

/// What this Package says about a generated identity, so a repair never lands one on an
/// existing conflict and never writes one nothing showed was free.
///
/// Asked of the canonical load, which only reads, so a repair that goes on to refuse has
/// rewritten nothing. Every commit precondition in this crate that consults a Package
/// reads it.
fn generated_identity_verdict<F: FileSystem>(
    plan: &SemanticRepairPlan,
    order_id: &str,
    filesystem: &F,
) -> GeneratedIdentityVerdict {
    let package = match load_every_record(&plan.root, filesystem) {
        Ok(package) => package,
        // Unmanaged, damaged, and outright failure alike: without a trustworthy view of
        // the Package an identity cannot be shown to be free, so it is refused rather
        // than assumed safe, under what the read established.
        Err(unusable) => return GeneratedIdentityVerdict::PackageUnusable(unusable),
    };
    // Every discovered Order counts, not only the healthy ones. Orders sharing an
    // identity are quarantined as damaged precisely because of that conflict, so
    // consulting the healthy set alone would declare the contested identity free.
    let taken = package
        .healthy_orders
        .iter()
        .any(|order| order.document.order_id.as_str() == order_id)
        || package.damaged_orders.iter().any(|order| {
            order
                .order_id
                .as_ref()
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

fn evidence_changed(plan: &SemanticRepairPlan, path: &Path) -> Failure {
    Failure::EvidenceChanged {
        evidence: rooted_at(&plan.root, path.to_path_buf()),
    }
}
