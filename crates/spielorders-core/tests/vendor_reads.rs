//! Reading one Vendor by the Vendor Name that selects it.
//!
//! The capability the CLI's `vendor` family consumes. Its reason for existing is checked
//! here rather than through an adapter: that the Name comparison a read resolves with is
//! the same one a create refuses a duplicate with — one comparison, so a Name cannot
//! reach a Vendor for one command and miss it for the other.
//!
//! Decision sources: Define Vendor identity and historical semantics; Define the Rust CLI
//! capability contract.

use crate::support::{QueuedUuids, RealFileSystem, initialized_package};
use spielorders_core::{
    mutation::MutationReport,
    vendor_reads::{VendorReadFailure, read_vendor_by_name},
    vendors::{
        VendorAssignments, VendorCapabilityError, commit_vendor_mutation, plan_vendor_creation,
    },
};
use std::path::Path;

const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";

/// One Vendor at a known identity, created through the capability that owns creating one.
fn created_vendor(root: &Path, name: &str, vendor_id: &str) {
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", name),
        &RealFileSystem,
    )
    .expect("the Vendor can be planned");
    assert!(matches!(
        commit_vendor_mutation(plan, &mut QueuedUuids::of(&[vendor_id]), &RealFileSystem),
        MutationReport::Committed { .. }
    ));
}

/// A Vendor Name resolves trimmed and without regard to letter case, beyond ASCII.
///
/// **The defect this catches is a resolution of its own.** An adapter — or a fourth
/// spelling here — that compared with `eq_ignore_ascii_case` or a naive lowercasing would
/// pass every test whose Vendor is called `Tarnwick` and fail the first Vendor whose Name
/// carries a diacritic: `vendor show` could not reach it, while `add-vendor` went on
/// refusing the same Name as a duplicate. The Names are package-unique under exactly one
/// comparison, and that is the claim below — the Name a read resolves is the Name a create
/// refuses.
#[test]
fn a_vendor_name_resolves_by_the_same_comparison_that_refuses_a_duplicate() {
    let (_tree, root) = initialized_package("vendor-read-name-comparison");
    created_vendor(&root, "Ångström Optik", VENDOR_ID);

    for spelling in [
        "Ångström Optik",
        "ångström optik",
        "ÅNGSTRÖM OPTIK",
        "  Ångström Optik  ",
    ] {
        let read = read_vendor_by_name(&root, spelling, &RealFileSystem)
            .unwrap_or_else(|failure| panic!("{spelling:?} resolves: {failure:?}"));
        assert_eq!(read.vendor().vendor_id.as_str(), VENDOR_ID, "{spelling:?}");
        assert_eq!(
            read.vendor().name.as_str(),
            "Ångström Optik",
            "{spelling:?}: the stored spelling is what is read back"
        );

        // The other half of the one comparison: the same spelling is a duplicate Name,
        // and the Vendor it duplicates is the one the read resolved.
        let refused = plan_vendor_creation(
            &root,
            &VendorAssignments::new().assign("name", spelling),
            &RealFileSystem,
        )
        .expect_err("the same spelling is a duplicate Vendor Name");
        let VendorCapabilityError::DuplicateName { vendor_id } = &refused else {
            panic!("{spelling:?} is refused as a duplicate Name: {refused:?}");
        };
        assert_eq!(vendor_id, VENDOR_ID, "{spelling:?}");
    }
}

/// A Name that matches no Vendor and a Package with no Vendors at all are two answers.
///
/// Both are failures — a read asked about one Vendor has none to report either way — and
/// they are typed apart because the action differs: check the Name, or create a Vendor.
/// Neither is an empty projection, which is what a caller must never have to distinguish
/// from a Vendor whose collections are genuinely empty.
#[test]
fn an_unresolvable_name_and_a_package_with_no_vendors_are_two_typed_answers() {
    let (_tree, root) = initialized_package("vendor-read-unresolvable");

    let failure = read_vendor_by_name(&root, "Nethermoor", &RealFileSystem)
        .expect_err("a Package with no Vendors resolves no Name");
    let VendorReadFailure::PackageHasNoVendors { name, context } = &failure else {
        panic!("a Package with no Vendors says so: {failure:?}");
    };
    assert_eq!(name, "Nethermoor");
    assert_eq!(
        context.root().absolute_path(),
        root,
        "the Root resolved and the Package loaded, which is how this is known"
    );

    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let failure = read_vendor_by_name(&root, "Nethermoor", &RealFileSystem)
        .expect_err("no Vendor of this Package carries that Name");
    let VendorReadFailure::VendorNameNotFound { name, .. } = &failure else {
        panic!("a Name matching no Vendor says so: {failure:?}");
    };
    assert_eq!(name, "Nethermoor");
}
