//! The two `vendor` read commands and the Name that selects their subject.
//!
//! **A Vendor command is Package-rooted**, because Vendors have no directories: `--root`
//! names the SpielOrders Root and `NAME` names the Vendor within it. The Name is resolved
//! by the established trimmed, case-insensitive, package-unique comparison — in the core,
//! which is the only place that comparison is spelled.
//!
//! Decision sources: Define the Rust CLI capability contract; Define Vendor identity and
//! historical semantics.

use crate::support::{
    HIDDEN_VENDOR_ID, Invocation, NEWEST_ORDER_ID, OFFERED_VENDOR_ID, OLDEST_ORDER_ID,
    directory_that_is_not_a_root, initialized_package, order_directory,
    package_with_a_damaged_order, package_with_an_unreferenced_vendor,
    package_with_duplicate_vendor_names, populated_package, response,
};

/// `vendor show` is the complete read of one Vendor: its shared facts, its Contacts
/// numbered from one, and the Orders that resolve here.
///
/// The defect this catches is an adapter that omitted an optional shared fact, numbered
/// the Contacts from zero, or reported the Orders referencing a Vendor as anything but
/// the ones whose records name it — each of which leaves a consumer unable to address a
/// Contact for the mutations that follow, or unable to see what a Vendor is used by.
#[test]
fn vendor_show_reports_the_complete_vendor_with_numbered_contacts_and_referencing_orders() {
    let (_tree, root) = populated_package("vendor-show-complete");

    let document = response(
        &Invocation::new(&["vendor", "show", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );

    assert_eq!(document["ok"], true);
    assert_eq!(document["contract"], "spielorders.vendor.show");
    assert_eq!(document["contract_version"], 1);
    // The machine response carries the Vendor ID the human surface holds back.
    assert_eq!(document["data"]["vendor_id"], OFFERED_VENDOR_ID);
    assert_eq!(document["data"]["name"], "Tarnwick");
    assert_eq!(document["data"]["phone"], "+1 555 0123");
    assert_eq!(document["data"]["email"], "sales@example.com");
    assert_eq!(document["data"]["website"], "https://example.com");
    assert_eq!(
        document["data"]["address"],
        "100 Example Avenue\nAnytown, ST 00000"
    );
    assert_eq!(
        document["data"]["purchasing_notes"],
        "Quote required above $5000."
    );
    assert_eq!(document["data"]["hidden"], false);

    let contacts = document["data"]["contacts"]
        .as_array()
        .expect("the Contacts are a present collection");
    assert_eq!(contacts.len(), 1);
    // One-based, and in the stored collection's own user-controlled order.
    assert_eq!(contacts[0]["position"], 1);
    assert_eq!(contacts[0]["name"], "Katherine Johnson");
    assert_eq!(contacts[0]["role"], "Accounts");
    assert_eq!(contacts[0]["email"], "accounts@example.com");
    assert!(contacts[0]["phone"].is_null());

    let referencing = document["data"]["referencing_orders"]
        .as_array()
        .expect("the referencing Orders are a present collection");
    assert_eq!(referencing.len(), 1);
    assert_eq!(referencing[0]["order_id"], NEWEST_ORDER_ID);
    assert_eq!(referencing[0]["title"], "Laser components");
    assert_eq!(referencing[0]["date_ordered"], "2026-08-22");
    assert_eq!(referencing[0]["status"], "in_preparation");
    let directory = order_directory(&root, "Laser_components");
    assert_eq!(
        referencing[0]["directory"]["package_relative_path"],
        directory
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("Orders/{name}"))
            .expect("the Order Directory has a UTF-8 name"),
        "a location inside a Package carries its Package-relative spelling too"
    );
}

/// A Name differing only in letter case or in surrounding whitespace resolves the same
/// Vendor, and the response reports the spelling the record stores.
///
/// The comparison is the Package's own and the core owns it. The defect this catches is an
/// adapter that compared the caller's text to the stored Name directly — which would make
/// `vendor show tarnwick` fail while `add-vendor "tarnwick"` was refused as a duplicate,
/// so a Name would resolve for one command and not for the other.
#[test]
fn a_name_differing_in_case_or_whitespace_resolves_the_same_vendor() {
    let (_tree, root) = populated_package("vendor-name-resolution");

    let canonical = response(
        &Invocation::new(&["vendor", "show", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );
    for spelling in ["tarnwick", "TARNWICK", "  Tarnwick  ", "tArNwIcK"] {
        let outcome = Invocation::new(&["vendor", "show"])
            .argument(spelling)
            .argument("--json")
            .rooted_at(&root)
            .run();
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(0), "{spelling:?}");
        assert_eq!(
            document["data"]["vendor_id"], OFFERED_VENDOR_ID,
            "{spelling:?} resolves the one Vendor whose Name it matches"
        );
        assert_eq!(
            document["data"]["name"], "Tarnwick",
            "{spelling:?}: the stored canonical spelling is what is reported"
        );
        assert_eq!(
            document["data"], canonical["data"],
            "{spelling:?} answers with the same Vendor, not a differently spelled one"
        );
    }
}

/// A Hidden Vendor answers both commands as any other does.
///
/// Hiding stops a Vendor being offered for new use and changes nothing about resolving
/// it: its facts, its Contacts, and its referencing Orders are reported, its visibility is
/// among the facts, and neither command is refused on visibility or reference grounds.
/// There is no `--all` here and none is needed — this is a resolution, not a listing.
#[test]
fn a_hidden_vendor_answers_as_any_other_does() {
    let (_tree, root) = populated_package("vendor-hidden");

    for command in [
        vec!["vendor", "show", "Nethermoor", "--json"],
        vec!["vendor", "contact", "list", "Nethermoor", "--json"],
    ] {
        let outcome = Invocation::new(&command).rooted_at(&root).run();
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(0), "{command:?}");
        assert_eq!(
            document["data"]["vendor_id"], HIDDEN_VENDOR_ID,
            "{command:?}"
        );
        assert_eq!(
            document["data"]["hidden"], true,
            "{command:?}: visibility is a reported fact"
        );
        assert_eq!(
            document["data"]["contacts"].as_array().map(Vec::len),
            Some(0),
            "{command:?}"
        );
        let referencing = document["data"]["referencing_orders"]
            .as_array()
            .expect("the referencing Orders are a present collection");
        assert_eq!(
            referencing.len(),
            1,
            "{command:?}: an Order references it and hiding does not hide that"
        );
        assert_eq!(referencing[0]["order_id"], OLDEST_ORDER_ID, "{command:?}");
    }

    // And the Order referencing it resolves it normally, which is the other half of
    // "hiding governs what is offered, never what is resolved".
    let order = response(
        &Invocation::new(&["order", "show", "--json"])
            .rooted_at(&order_directory(&root, "Mirror_mounts"))
            .run(),
    );
    assert_eq!(order["data"]["vendor"]["name"], "Nethermoor");
    assert_eq!(order["data"]["vendor"]["hidden"], true);
}

/// A Vendor with no Contacts and nothing referencing it answers with present, empty
/// collections.
///
/// The defect this catches is an omitted key: a consumer written against every other
/// Vendor would have to null-check `contacts` and `referencing_orders` for this one, and
/// once one command omits an empty collection the rule is gone. The optional shared facts
/// are `null` for the same reason — absent, and said so.
#[test]
fn a_vendor_with_no_contacts_and_no_referencing_orders_answers_with_empty_collections() {
    let (_tree, root) = package_with_an_unreferenced_vendor("vendor-empty-collections");

    for command in [
        vec!["vendor", "show", "Nethermoor", "--json"],
        vec!["vendor", "contact", "list", "Nethermoor", "--json"],
    ] {
        let outcome = Invocation::new(&command).rooted_at(&root).run();
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(0), "{command:?}");
        assert_eq!(document["data"]["name"], "Nethermoor", "{command:?}");
        assert_eq!(
            document["data"]["contacts"].as_array().map(Vec::len),
            Some(0),
            "{command:?}: present and empty, not omitted"
        );
        assert_eq!(
            document["data"]["referencing_orders"]
                .as_array()
                .map(Vec::len),
            Some(0),
            "{command:?}: present and empty, not omitted"
        );
        for fact in ["phone", "email", "website", "address", "purchasing_notes"] {
            assert!(
                document["data"][fact].is_null(),
                "{command:?}: an absent optional fact is null: {fact}"
            );
        }
        assert_eq!(document["data"]["hidden"], false, "{command:?}");
    }

    // The empty collections are an answer, and the human surface says so too rather than
    // leaving a reader unable to tell an empty list from one the summary forgot.
    let printed = Invocation::new(&["vendor", "show", "Nethermoor"])
        .rooted_at(&root)
        .run();
    assert!(
        printed.stdout_text().contains("Contacts: none"),
        "{}",
        printed.stdout_text()
    );
    assert!(
        printed.stdout_text().contains("Referencing Orders: none"),
        "{}",
        printed.stdout_text()
    );
}

/// A Name no Vendor carries is a typed failure about that Name, and never an empty
/// answer.
///
/// **A failure rather than an empty result**, because `vendor show` was asked about one
/// Vendor: `ok` is false, `data` is `null`, and the Name the caller supplied is what the
/// error is about. The Name and not a Vendor ID, because a Name that resolved to nothing
/// has none — Names are the selector and identities are returned.
///
/// A Package that has no Vendors at all is the same question with a different action
/// behind it, so it carries its own code: create a Vendor rather than check the Name.
#[test]
fn an_unresolvable_name_fails_about_the_name_rather_than_answering_emptily() {
    let (_tree, populated) = populated_package("vendor-name-unresolvable");
    let (_empty_tree, without_vendors) = initialized_package("vendor-none-at-all");

    for command in [
        vec!["vendor", "show", "Copperwick", "--json"],
        vec!["vendor", "contact", "list", "Copperwick", "--json"],
    ] {
        let outcome = Invocation::new(&command).rooted_at(&populated).run();
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(1), "{command:?}");
        assert_eq!(document["ok"], false, "{command:?}");
        assert!(
            document["data"].is_null(),
            "{command:?}: a Name that matched nothing is not an empty Vendor"
        );
        assert_eq!(document["error"]["code"], "vendor_not_found", "{command:?}");
        assert_eq!(document["error"]["scope"], "package", "{command:?}");
        assert_eq!(
            document["error"]["identities"][0]["kind"], "vendor_name",
            "{command:?}"
        );
        assert_eq!(
            document["error"]["identities"][0]["value"], "Copperwick",
            "{command:?}"
        );
        // The Vendor record is the document that holds no such Vendor, and the Package
        // resolved — so the context names what the command was about.
        assert_eq!(
            document["error"]["paths"][0]["package_relative_path"], ".spielorders/vendors.json",
            "{command:?}"
        );
        assert_eq!(
            document["context"]["package_root"],
            populated.to_str().expect("the test root is UTF-8"),
            "{command:?}"
        );

        // The same Name against a Package holding no Vendors: also a failure, and a
        // distinguishable one.
        let empty = Invocation::new(&command).rooted_at(&without_vendors).run();
        let document = response(&empty);
        assert_eq!(empty.status, Some(1), "{command:?}");
        assert_eq!(
            document["error"]["code"], "package_has_no_vendors",
            "{command:?}: a Package with no Vendors is its own answer"
        );
        assert_eq!(
            document["error"]["identities"][0]["value"], "Copperwick",
            "{command:?}"
        );
    }
}

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
        text.contains("Contact: 1. Katherine Johnson  Accounts  accounts@example.com"),
        "{text}"
    );
    // Not the Vendor's own shared facts, and not what references it: those are the
    // complete read, and this command renders the Contacts of it.
    assert!(!text.contains("sales@example.com"), "{text}");
    assert!(!text.contains("Website"), "{text}");
    assert!(!text.contains("Referencing Order"), "{text}");
}

/// An Order that could not be read names no Vendor, so the answer is partial and says so.
///
/// **The observation is the point.** A Damaged Order's record is unreadable, so whether it
/// references this Vendor is unknown and it appears in no referencing summary — and
/// without the load's diagnostic beside it, that shortened list would be indistinguishable
/// from a Vendor nothing references. The diagnostics reach the response from the one load
/// that produced the answer, not from a second one.
#[test]
fn a_damaged_order_leaves_the_referencing_list_partial_and_the_reason_beside_it() {
    let (_tree, root, _damaged) = package_with_a_damaged_order("vendor-partial-referencing");

    let outcome = Invocation::new(&["vendor", "show", "Nethermoor", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(
        outcome.status,
        Some(0),
        "one Damaged Order does not damage the Package: {}",
        outcome.stdout_text()
    );
    assert_eq!(
        document["data"]["referencing_orders"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "the only Order referencing this Vendor is the one that could not be read"
    );
    let diagnostics = document["diagnostics"]
        .as_array()
        .expect("diagnostics is a present collection");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["scope"], "order");
    assert_eq!(
        document["ok"], true,
        "which is a partial answer, not a failure"
    );

    // A warning-severity observation goes to standard error while the result goes to
    // standard output.
    let human = Invocation::new(&["vendor", "show", "Nethermoor"])
        .rooted_at(&root)
        .run();
    assert_eq!(human.status, Some(0), "{}", human.stderr_text());
    assert!(
        human.stdout_text().contains("Referencing Orders: none"),
        "{}",
        human.stdout_text()
    );
    assert_ne!(human.stderr, b"", "the load's observation is reported");
}

/// A `vendor` command is Package-rooted, and it classifies a Package exactly as a
/// Package-level command does.
///
/// `--root` names the SpielOrders Root exactly and no ancestor of it is consulted, so a
/// plain directory inside a Package fails rather than finding the Package one level up.
/// The codes are the shared ones: a `vendor` command and a `package` command disagreeing
/// about an unmanaged or a damaged Package is a defect that appears in neither command's
/// own diff.
#[test]
fn a_vendor_read_targets_the_package_root_exactly_and_shares_its_load_states() {
    let (_tree, root) = populated_package("vendor-root-targeting");
    let inside = directory_that_is_not_a_root(&root);
    let (_damaged_tree, damaged) = package_with_duplicate_vendor_names("vendor-root-damaged");

    let unmanaged = Invocation::new(&["vendor", "show", "Tarnwick", "--json"])
        .rooted_at(&inside)
        .run();
    let document = response(&unmanaged);
    assert_eq!(unmanaged.status, Some(1));
    assert_eq!(document["error"]["code"], "package_root_unmanaged");
    assert!(
        document["context"]["package_root"].is_null(),
        "an unmanaged directory is a resolved path, not a Root"
    );
    assert_eq!(
        document["error"]["paths"][0]["path"],
        inside.to_str().expect("the test path is UTF-8")
    );

    let unreadable = Invocation::new(&["vendor", "show", "Tarnwick", "--json"])
        .rooted_at(&damaged)
        .run();
    let document = response(&unreadable);
    assert_eq!(unreadable.status, Some(1));
    assert_eq!(document["error"]["code"], "package_root_damaged");
    assert_eq!(
        document["context"]["package_root"],
        damaged.to_str().expect("the test root is UTF-8"),
        "the Root resolved and what stands under it could not be trusted"
    );
    assert_eq!(
        document["diagnostics"][0]["code"], "vendor_name_duplicate",
        "the diagnosis travels in the only channel a failed read has"
    );
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
            !human.stdout_text().contains(OFFERED_VENDOR_ID),
            "{command:?}: {}",
            human.stdout_text()
        );

        let verbose = Invocation::new(&command)
            .argument("--verbose")
            .rooted_at(&root)
            .run();
        assert_eq!(verbose.status, Some(0), "{command:?}");
        assert!(
            verbose.stdout_text().contains(OFFERED_VENDOR_ID),
            "{command:?}: the Vendor ID is `--verbose` detail: {}",
            verbose.stdout_text()
        );
        assert!(
            verbose
                .stdout_text()
                .contains(root.to_str().expect("the test root is UTF-8")),
            "{command:?}: which Package the read is of is the same detail for both: {}",
            verbose.stdout_text()
        );
    }
}
