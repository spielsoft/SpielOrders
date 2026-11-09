//! Reading one Vendor by the Vendor Name that selects it.
//!
//! The capability the CLI's `vendor` family consumes. Its reason for existing is checked
//! here rather than through an adapter: that the Name comparison a read resolves with is
//! the same one a create refuses a duplicate with — one comparison, so a Name cannot
//! reach a Vendor for one command and miss it for the other.
//!
//! Decision sources: Define Vendor identity and historical semantics; Define the Rust CLI
//! capability contract.

use crate::Assignments;
use crate::Failure;
use crate::tests::support::{
    PlatformFileSystem, create_vendor, created_vendor, effects, initialized_package, vendor_named,
};
use std::ops::Deref;

const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";

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
        let read = vendor_named(&root, spelling, &PlatformFileSystem)
            .result
            .unwrap_or_else(|failure| panic!("{spelling:?} resolves: {failure:?}"));
        assert_eq!(read.vendor.vendor_id.deref(), VENDOR_ID, "{spelling:?}");
        assert_eq!(
            read.vendor.name.deref(),
            "Ångström Optik",
            "{spelling:?}: the stored spelling is what is read back"
        );

        // The other half of the one comparison: the same spelling is a duplicate Name,
        // and the Vendor it duplicates is the one the read resolved.
        let refused = crate::plan(
            &create_vendor(root.clone(), Assignments::new().assign("name", spelling)),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect_err("the same spelling is a duplicate Vendor Name");
        let Failure::VendorNameInUse { vendor_id, .. } = &refused else {
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

    let failure = vendor_named(&root, "Nethermoor", &PlatformFileSystem)
        .result
        .expect_err("a Package with no Vendors resolves no Name");
    let Failure::PackageHasNoVendors { name, context } = &failure else {
        panic!("a Package with no Vendors says so: {failure:?}");
    };
    assert_eq!(name, "Nethermoor");
    assert_eq!(
        context.root.absolute_path, root,
        "the Root resolved and the Package loaded, which is how this is known"
    );

    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let failure = vendor_named(&root, "Nethermoor", &PlatformFileSystem)
        .result
        .expect_err("no Vendor of this Package carries that Name");
    let Failure::UnknownVendor { name, .. } = &failure else {
        panic!("a Name matching no Vendor says so: {failure:?}");
    };
    assert_eq!(name, "Nethermoor");
}
