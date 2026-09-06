//! Maintaining one Vendor's ordered Contacts by one-based position.
//!
//! **The number a reader sees is the number a mutation takes**, which is what these four
//! commands are for: `vendor show` and `vendor contact list` number a Vendor's Contacts
//! from one, and `edit`, `move`, and `remove` address exactly those numbers against the
//! collection as it stood before the mutation.
//!
//! Decision sources: Define the Rust CLI capability contract; Define Vendor identity and
//! historical semantics; Durable managed-file writes.

use crate::support::{
    Invocation, managed_snapshot, package_with_three_contacts, populated_package, response,
};

/// The Contact Names of one Vendor, in the order the response reports them, each with the
/// position it is reported at.
fn contacts(root: &std::path::Path, name: &str) -> Vec<(u64, String)> {
    let read = response(
        &Invocation::new(&["vendor", "contact", "list"])
            .argument(name)
            .argument("--json")
            .rooted_at(root)
            .run(),
    );
    read["data"]["contacts"]
        .as_array()
        .expect("the Contacts are a list")
        .iter()
        .map(|contact| {
            (
                contact["position"].as_u64().expect("a one-based position"),
                contact["name"].as_str().expect("a Contact Name").to_owned(),
            )
        })
        .collect()
}

/// `add` appends unless `position=` is supplied, and `position=1` inserts first.
///
/// **`position=` is not a Contact fact**, which is the defect this catches: an adapter that
/// passed it through with the Contact's own assignments would have the core refuse it as an
/// unknown field, and one that read it and forgot to shift would overwrite the Contact
/// already standing there.
#[test]
fn add_appends_unless_a_position_is_supplied() {
    let (_tree, root) = populated_package("contact-add");

    let appended = Invocation::new(&["vendor", "contact", "add", "Tarnwick"])
        .argument("name=Grace Hopper")
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_eq!(appended.status, Some(0), "{}", appended.stderr_text());
    let appended = response(&appended);
    assert_eq!(appended["data"]["report"]["outcome"], "added");
    assert_eq!(appended["data"]["report"]["destination_position"], 2);
    assert!(
        appended["data"]["report"]["source_position"].is_null(),
        "an appended Contact came from no position"
    );
    assert_eq!(
        contacts(&root, "Tarnwick"),
        vec![
            (1, "Katherine Johnson".to_owned()),
            (2, "Grace Hopper".to_owned()),
        ],
        "the Contact was appended"
    );

    let inserted = Invocation::new(&["vendor", "contact", "add", "Tarnwick"])
        .argument("name=Alan Turing")
        .argument("position=1")
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_eq!(inserted.status, Some(0), "{}", inserted.stderr_text());
    assert_eq!(
        response(&inserted)["data"]["report"]["destination_position"],
        1
    );
    assert_eq!(
        contacts(&root, "Tarnwick"),
        vec![
            (1, "Alan Turing".to_owned()),
            (2, "Katherine Johnson".to_owned()),
            (3, "Grace Hopper".to_owned()),
        ],
        "an inserted Contact shifts the rest rather than replacing one"
    );

    // The range an insertion may name is one number longer than the collection, because
    // the place after the last Contact is a place a Contact can go and is not one any
    // Contact occupies. One past *that* names nowhere at all.
    let after_the_last = Invocation::new(&["vendor", "contact", "add", "Tarnwick"])
        .argument("name=Katherine G. Johnson")
        .argument("position=4")
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_eq!(
        after_the_last.status,
        Some(0),
        "{}",
        after_the_last.stderr_text()
    );
    assert_eq!(
        response(&after_the_last)["data"]["report"]["destination_position"],
        4
    );
    let past_the_end = Invocation::new(&["vendor", "contact", "add", "Tarnwick"])
        .argument("name=Ada Lovelace")
        .argument("position=6")
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_eq!(past_the_end.status, Some(1));
    assert_eq!(
        response(&past_the_end)["error"]["code"],
        "vendor_contact_position_invalid"
    );
    assert_eq!(
        contacts(&root, "Tarnwick").len(),
        4,
        "the refused insertion added nothing"
    );
}

/// `edit`, `move`, and `remove` at position N act on the Contact that was at N *before* the
/// mutation.
///
/// **The defect this catches is an off-by-one between the number a reader sees and the
/// number a mutation takes**, which is exactly what a contract about positions is for: each
/// of the three is asked for the middle Contact of three, and each is proven by the entry
/// that is no longer where it was. A collection of one would make every position name the
/// same Contact, so this asks a collection where the three positions are three answers.
#[test]
fn every_position_addresses_the_pre_mutation_collection() {
    let before = vec![
        (1, "Katherine Johnson".to_owned()),
        (2, "Grace Hopper".to_owned()),
        (3, "Alan Turing".to_owned()),
    ];

    let (_edited_tree, edited_root) = package_with_three_contacts("contact-position-edit");
    assert_eq!(contacts(&edited_root, "Tarnwick"), before);
    let edited = Invocation::new(&["vendor", "contact", "edit", "Tarnwick", "2"])
        .argument("name=Grace B. Hopper")
        .argument("--json")
        .rooted_at(&edited_root)
        .run();
    assert_eq!(edited.status, Some(0), "{}", edited.stderr_text());
    let edited_document = response(&edited);
    assert_eq!(edited_document["data"]["report"]["outcome"], "edited");
    assert_eq!(edited_document["data"]["report"]["source_position"], 2);
    assert_eq!(
        edited_document["data"]["report"]["contact"]["name"],
        "Grace B. Hopper"
    );
    assert_eq!(
        contacts(&edited_root, "Tarnwick"),
        vec![
            (1, "Katherine Johnson".to_owned()),
            (2, "Grace B. Hopper".to_owned()),
            (3, "Alan Turing".to_owned()),
        ],
        "the second Contact was edited and the collection order is unchanged"
    );

    let (_moved_tree, moved_root) = package_with_three_contacts("contact-position-move");
    let moved = Invocation::new(&["vendor", "contact", "move", "Tarnwick", "2"])
        .argument("position=1")
        .argument("--json")
        .rooted_at(&moved_root)
        .run();
    assert_eq!(moved.status, Some(0), "{}", moved.stderr_text());
    let moved_document = response(&moved);
    assert_eq!(moved_document["data"]["report"]["outcome"], "moved");
    assert_eq!(moved_document["data"]["report"]["source_position"], 2);
    assert_eq!(moved_document["data"]["report"]["destination_position"], 1);
    assert_eq!(
        contacts(&moved_root, "Tarnwick"),
        vec![
            (1, "Grace Hopper".to_owned()),
            (2, "Katherine Johnson".to_owned()),
            (3, "Alan Turing".to_owned()),
        ],
        "the second Contact became the first and the one it displaced moved down"
    );

    let (_removed_tree, removed_root) = package_with_three_contacts("contact-position-remove");
    let removed = Invocation::new(&["vendor", "contact", "remove", "Tarnwick", "2", "--json"])
        .rooted_at(&removed_root)
        .run();
    assert_eq!(removed.status, Some(0), "{}", removed.stderr_text());
    let removed_document = response(&removed);
    assert_eq!(removed_document["data"]["report"]["outcome"], "removed");
    assert_eq!(removed_document["data"]["report"]["source_position"], 2);
    assert!(
        removed_document["data"]["report"]["destination_position"].is_null(),
        "a removed Contact was left nowhere"
    );
    assert_eq!(
        removed_document["data"]["report"]["contact"]["name"], "Grace Hopper",
        "the report names the Contact that is no longer in the collection"
    );
    assert_eq!(
        contacts(&removed_root, "Tarnwick"),
        vec![
            (1, "Katherine Johnson".to_owned()),
            (2, "Alan Turing".to_owned()),
        ],
        "the second Contact went and the third took its number"
    );
}

/// Every way of naming no position is one refused mutation at exit `1`, with the position
/// named and nothing written.
///
/// **One code for all four**, because they are one mistake: a position addresses a Contact
/// of a real collection, so a caller correcting `abc`, `-1`, `0`, and `9` acts on the same
/// thing and branches once. The identity says which end of the mutation the position was
/// for, which is the fact a `move` needs and a message cannot carry — human wording is not
/// a parsing contract.
#[test]
fn every_position_that_names_no_contact_is_refused_without_writing() {
    let (_tree, root) = package_with_three_contacts("contact-position-refusals");
    let before = managed_snapshot(&root);

    // Each command line is complete, so the run reaches the position under test rather
    // than a missing argument — and the last two are the destination end, which is the
    // whole reason the identity carries a role.
    for (command, role, named) in [
        (
            vec!["vendor", "contact", "edit", "Tarnwick", "0"],
            "source_position",
            "0",
        ),
        (
            vec!["vendor", "contact", "edit", "Tarnwick", "-1"],
            "source_position",
            "-1",
        ),
        (
            vec!["vendor", "contact", "edit", "Tarnwick", "abc"],
            "source_position",
            "abc",
        ),
        (
            vec!["vendor", "contact", "edit", "Tarnwick", "4"],
            "source_position",
            "4",
        ),
        (
            vec!["vendor", "contact", "remove", "Tarnwick", "9"],
            "source_position",
            "9",
        ),
        (
            vec!["vendor", "contact", "move", "Tarnwick", "9", "position=1"],
            "source_position",
            "9",
        ),
        (
            vec!["vendor", "contact", "move", "Tarnwick", "1", "position=9"],
            "destination_position",
            "9",
        ),
        (
            vec![
                "vendor",
                "contact",
                "add",
                "Tarnwick",
                "name=Ada Lovelace",
                "position=0",
            ],
            "destination_position",
            "0",
        ),
    ] {
        let outcome = Invocation::new(&command)
            .argument("--json")
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{command:?}");
        assert_eq!(document["ok"], false, "{command:?}");
        assert!(document["data"].is_null(), "{command:?}");
        assert_eq!(
            document["error"]["code"], "vendor_contact_position_invalid",
            "{command:?}"
        );
        assert_eq!(
            document["error"]["identities"][0]["kind"], role,
            "{command:?}: the identity says which end the position was for"
        );
        assert_eq!(
            document["error"]["identities"][0]["value"], named,
            "{command:?}: and quotes the position as the command line wrote it"
        );
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{command:?}: a refused position writes nothing"
        );
    }
}

/// A `move` to the position a Contact already occupies changes nothing, and that is a
/// success.
///
/// The idempotent no-op, which is exit `0` by the decided rule. The defect this catches is
/// a command that refused it — a caller reordering a collection would then have to know
/// where each Contact already is before asking for the order it wants.
#[test]
fn a_move_to_the_same_position_changes_nothing_and_succeeds() {
    let (_tree, root) = package_with_three_contacts("contact-move-no-op");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["vendor", "contact", "move", "Tarnwick", "2"])
        .argument("position=2")
        .argument("--json")
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(document["data"]["report"]["committed"], true);
    assert_eq!(
        document["data"]["report"]["changed"], false,
        "nothing was written, and that is a success"
    );
    assert_eq!(
        document["data"]["report"]["operations"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "and it announced no managed operation"
    );
    assert_eq!(
        document["data"]["report"]["index"]["refresh"], "not_attempted",
        "a commit that wrote nothing refreshed nothing either"
    );
    assert_eq!(managed_snapshot(&root), before);
}

/// A Contact requires a Name, and an empty value clears an optional fact into absence.
///
/// **The three refusals are three different mistakes**, and a caller correcting them acts
/// on different things: a Name assigned nothing but whitespace is a refused assignment
/// naming the field and the rule, a command line that assigned no Name at all is the
/// Contact's own missing-Name refusal — not the Vendor's, whose Name resolved this command
/// — and neither writes. The clearing half is the reverse claim for the optional facts: an
/// empty value is how a role, a phone, or an email goes away, and the defect it catches is
/// a fact stored as an empty string, which reads back as present and blank.
#[test]
fn a_contact_requires_a_name_and_an_empty_value_clears_an_optional_fact() {
    let (_tree, root) = populated_package("contact-required-name");
    let before = managed_snapshot(&root);

    for (command, code) in [
        (
            vec!["vendor", "contact", "add", "Tarnwick", "name=   "],
            "vendor_assignment_rejected",
        ),
        (
            vec!["vendor", "contact", "edit", "Tarnwick", "1", "name="],
            "vendor_assignment_rejected",
        ),
        (
            vec!["vendor", "contact", "add", "Tarnwick", "role=Shipping"],
            "vendor_contact_name_required",
        ),
    ] {
        let outcome = Invocation::new(&command)
            .argument("--json")
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{command:?}");
        assert!(document["data"].is_null(), "{command:?}");
        assert_eq!(document["error"]["code"], code, "{command:?}");
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{command:?}: a Contact without a Name is written nowhere"
        );
    }

    let cleared = Invocation::new(&["vendor", "contact", "edit", "Tarnwick", "1", "role="])
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_eq!(cleared.status, Some(0), "{}", cleared.stderr_text());
    let cleared = response(&cleared);
    assert_eq!(cleared["data"]["report"]["changed"], true);
    assert!(
        cleared["data"]["vendor"]["contacts"][0]["role"].is_null(),
        "the cleared fact is absent: {}",
        cleared["data"]["vendor"]["contacts"][0]
    );

    // Read back through the command whose contract is about reading, because a fact that
    // is absent in the mutation's answer and empty on disk is the defect this is about.
    let read = response(
        &Invocation::new(&["vendor", "contact", "list", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert!(read["data"]["contacts"][0]["role"].is_null());
    assert_eq!(
        read["data"]["contacts"][0]["email"], "accounts@example.com",
        "and no other fact was cleared"
    );
}

/// A Vendor's last Contact may be removed, leaving a collection that is present and empty.
///
/// **Present and empty rather than absent**, which is the whole claim: a consumer that
/// read `contacts` as a list keeps reading it as one, and a Vendor emptied of Contacts
/// answers exactly as a Vendor created without any does. The defect this catches is a
/// removal that dropped the member, or refused the last Contact to avoid the question.
#[test]
fn a_vendors_last_contact_may_go_leaving_a_present_empty_collection() {
    let (_tree, root) = populated_package("contact-last-removal");
    assert_eq!(contacts(&root, "Tarnwick").len(), 1, "the sole Contact");

    let outcome = Invocation::new(&["vendor", "contact", "remove", "Tarnwick", "1", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    let document = response(&outcome);

    assert_eq!(document["data"]["report"]["outcome"], "removed");
    assert_eq!(
        document["data"]["vendor"]["contacts"].as_array(),
        Some(&Vec::new()),
        "the collection is present and empty"
    );
    let read = response(
        &Invocation::new(&["vendor", "contact", "list", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(read["data"]["contacts"].as_array(), Some(&Vec::new()));
}

/// Each of the four planned rather than performed: the real run's outcome, `committed:
/// false`, and a Package left byte-identical.
///
/// **The plan is compared against the commit of the same command line**, which is what
/// makes this more than an assertion that nothing was written: a dry run that reported a
/// different Contact, a different position, or a different collection would be a
/// description of a mutation nobody can perform. `committed` is the one member that
/// differs.
#[test]
fn every_dry_run_reports_the_real_runs_outcome_and_changes_nothing() {
    for (label, command, outcome_code) in [
        (
            "contact-dry-run-add",
            vec!["vendor", "contact", "add", "Tarnwick", "name=Ada Lovelace"],
            "added",
        ),
        (
            "contact-dry-run-edit",
            vec![
                "vendor",
                "contact",
                "edit",
                "Tarnwick",
                "2",
                "phone=+1 555 0199",
            ],
            "edited",
        ),
        (
            "contact-dry-run-move",
            vec!["vendor", "contact", "move", "Tarnwick", "3", "position=1"],
            "moved",
        ),
        (
            "contact-dry-run-remove",
            vec!["vendor", "contact", "remove", "Tarnwick", "2"],
            "removed",
        ),
    ] {
        let (_tree, root) = package_with_three_contacts(label);
        let before = managed_snapshot(&root);

        let planned = Invocation::new(&command)
            .argument("--json")
            .argument("--dry-run")
            .rooted_at(&root)
            .run();
        assert_eq!(
            planned.status,
            Some(0),
            "{label}: {}",
            planned.stderr_text()
        );
        let planned = response(&planned);

        assert_eq!(planned["data"]["report"]["committed"], false, "{label}");
        assert_eq!(
            planned["data"]["report"]["outcome"], outcome_code,
            "{label}"
        );
        assert_eq!(planned["data"]["report"]["changed"], true, "{label}");
        assert_eq!(
            planned["data"]["report"]["index"]["refresh"], "not_attempted",
            "{label}: a run that never committed refreshed nothing"
        );
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{label}: no managed file, no staging artifact"
        );

        let committed = Invocation::new(&command)
            .argument("--json")
            .rooted_at(&root)
            .run();
        assert_eq!(
            committed.status,
            Some(0),
            "{label}: {}",
            committed.stderr_text()
        );
        let committed = response(&committed);

        assert_eq!(committed["data"]["report"]["committed"], true, "{label}");
        assert_eq!(
            planned["data"]["vendor"], committed["data"]["vendor"],
            "{label}: a plan reports the collection the commit produces"
        );
        for member in [
            "changed",
            "outcome",
            "contact",
            "source_position",
            "destination_position",
            "operations",
            "warnings",
        ] {
            assert_eq!(
                planned["data"]["report"][member], committed["data"]["report"][member],
                "{label}: `{member}` is the same question of a plan and of a commit"
            );
        }
    }
}

/// A Hidden Vendor's Contacts are mutable exactly as a visible Vendor's are.
///
/// **Hiding is about whether a Vendor is offered for new use, not about whether its record
/// may be maintained**: the Orders that reference it still resolve to it, so a stale phone
/// number on one of its Contacts is a thing a user needs to be able to correct. The defect
/// this catches is a Contact mutation that refused a Hidden Vendor, or one that quietly
/// unhid it — so visibility is asserted on the way out of every one of the four.
#[test]
fn a_hidden_vendors_contacts_are_mutable_as_a_visible_vendors_are() {
    let (_tree, root) = populated_package("hidden-vendor-contacts");
    let hidden = "Nethermoor";

    for command in [
        vec![
            "vendor",
            "contact",
            "add",
            hidden,
            "name=Grace Hopper",
            "phone=+1 555 0142",
        ],
        vec!["vendor", "contact", "add", hidden, "name=Alan Turing"],
        // Grace Hopper is the Contact with a phone to clear, and the move is what puts her
        // at the position the last two address.
        vec!["vendor", "contact", "move", hidden, "1", "position=2"],
        vec!["vendor", "contact", "edit", hidden, "2", "phone="],
        vec!["vendor", "contact", "remove", hidden, "2"],
    ] {
        let outcome = Invocation::new(&command)
            .argument("--json")
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(0),
            "{command:?}: {}",
            outcome.stderr_text()
        );
        assert_eq!(document["data"]["report"]["changed"], true, "{command:?}");
        assert_eq!(
            document["data"]["vendor"]["hidden"], true,
            "{command:?}: maintaining a Contact does not offer the Vendor again"
        );
    }

    assert_eq!(
        contacts(&root, hidden),
        vec![(1, "Alan Turing".to_owned())],
        "the four mutations left the collection they say they did"
    );
}
