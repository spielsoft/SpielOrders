//! The two `vendor` read commands and the Name that selects their subject.
//!
//! **A Vendor command is Package-rooted**, because Vendors have no directories: `--root`
//! names the SpielOrders Root and `NAME` names the Vendor within it. The Name is resolved
//! by the established trimmed, case-insensitive, package-unique comparison — in the core,
//! which is the only place that comparison is spelled.
//!
//! Decision sources: Define the Rust CLI capability contract; Define Vendor identity and
//! historical semantics.

use crate::support::{Invocation, offered_vendor_id, populated_package, response};

/// `vendor contact list` answers with `vendor show`'s document, byte for byte.
///
/// Asserted on the bytes rather than on parsed members, because the claim is identity and
/// not agreement: a member added to one and not the other would still agree everywhere a
/// member-by-member comparison thought to look.
#[test]
fn vendor_contact_list_answers_with_vendor_shows_document() {
    let (_tree, root) = populated_package("vendor-contact-list-shares-the-contract");

    let listing = Invocation::new(&["vendor", "contact", "list", "Tarnwick", "--json"])
        .rooted_at(&root)
        .run();
    let show = Invocation::new(&["vendor", "show", "Tarnwick", "--json"])
        .rooted_at(&root)
        .run();

    // Both succeeded, before any claim that they agree: two identical failures would
    // satisfy every assertion below without either command having answered.
    assert_eq!(listing.status, Some(0), "{}", listing.stdout_text());
    assert_eq!(show.status, Some(0), "{}", show.stdout_text());
    assert_eq!(listing.stdout_text(), show.stdout_text());
    assert_eq!(
        response(&listing)["contract"],
        "spielorders.vendor.show",
        "the sub-list carries the projection's identity, not one of its own"
    );

    // And on a failure, which is the response a sub-list is most likely to answer its own
    // way: a Name that resolved to nothing has no Contacts to render, and the document is
    // still the one `vendor show` emits.
    let failed_listing = Invocation::new(&["vendor", "contact", "list", "Copperwick", "--json"])
        .rooted_at(&root)
        .run();
    let failed_show = Invocation::new(&["vendor", "show", "Copperwick", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(failed_listing.status, Some(1));
    assert_eq!(failed_show.status, Some(1));
    assert_eq!(failed_listing.stdout_text(), failed_show.stdout_text());
}

/// The whole difference between the two command lines: the human surface renders the
/// Contacts and not the Vendor's other facts.
#[test]
fn the_human_contact_list_renders_the_contacts_alone() {
    let (_tree, root) = populated_package("vendor-contact-list-human");

    let printed = Invocation::new(&["vendor", "contact", "list", "Tarnwick"])
        .rooted_at(&root)
        .run();
    let text = printed.stdout_text();

    assert_eq!(printed.status, Some(0), "{}", printed.stderr_text());
    assert!(
        text.contains(
            "1. name: Katherine Johnson\n  role: Accounts\n  email: accounts@example.com"
        ),
        "{text}"
    );
    // Not the Vendor's own shared facts, and not what references it: those are the
    // complete read, and this command renders the Contacts of it.
    assert!(!text.contains("sales@example.com"), "{text}");
    assert!(!text.contains("website"), "{text}");
    assert!(!text.contains("referencing orders"), "{text}");
}

/// `--quiet` says nothing on success and `--verbose` adds the detail, for both commands.
///
/// The four output modes are one set, so a command that answered under `--quiet` or that
/// leaked its result into the wrong stream would be a hole in a rule fifteen other
/// commands keep. UUIDs are `--verbose` detail: a human reading a Vendor wants its Name.
#[test]
fn the_four_output_modes_hold_for_both_vendor_reads() {
    let (_tree, root) = populated_package("vendor-output-modes");

    for command in [
        vec!["vendor", "show", "Tarnwick"],
        vec!["vendor", "contact", "list", "Tarnwick"],
    ] {
        let quiet = Invocation::new(&command)
            .argument("--quiet")
            .rooted_at(&root)
            .run();
        assert_eq!(quiet.status, Some(0), "{command:?}");
        assert_eq!(quiet.stdout, b"", "{command:?}: {}", quiet.stdout_text());
        assert_eq!(quiet.stderr, b"", "{command:?}: {}", quiet.stderr_text());

        let human = Invocation::new(&command).rooted_at(&root).run();
        assert!(
            !human.stdout_text().contains(&*offered_vendor_id(&root)),
            "{command:?}: {}",
            human.stdout_text()
        );

        let verbose = Invocation::new(&command)
            .argument("--verbose")
            .rooted_at(&root)
            .run();
        assert_eq!(verbose.status, Some(0), "{command:?}");
        if command[1] == "contact" {
            // `vendor contact list` renders the Contacts alone, under every mode.
            assert!(
                verbose.stdout_text().contains("1. name: Katherine Johnson"),
                "{command:?}: {}",
                verbose.stdout_text()
            );
            continue;
        }
        assert!(
            verbose.stdout_text().contains(&*offered_vendor_id(&root)),
            "{command:?}: the Vendor ID is `--verbose` detail: {}",
            verbose.stdout_text()
        );
    }
}
