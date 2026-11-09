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
/// one-based position that order gives it.
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
        .zip(1..)
        .map(|(contact, position)| {
            (
                position,
                contact["name"].as_str().expect("a Contact Name").to_owned(),
            )
        })
        .collect()
}

/// The position a mutation's plan reports a row added at.
fn added(document: &serde_json::Value) -> Option<u64> {
    document["data"]["changes"]
        .as_array()?
        .iter()
        .find(|change| change["kind"] == "row_added")?["position"]
        .as_u64()
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
    assert_eq!(added(&appended), Some(2));
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
    assert_eq!(added(&response(&inserted)), Some(1));
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
    assert_eq!(added(&response(&after_the_last)), Some(4));
    let past_the_end = Invocation::new(&["vendor", "contact", "add", "Tarnwick"])
        .argument("name=Ada Lovelace")
        .argument("position=6")
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_eq!(past_the_end.status, Some(1));
    assert_eq!(
        response(&past_the_end)["error"]["code"],
        "invalid_contact_position"
    );
    assert_eq!(
        contacts(&root, "Tarnwick").len(),
        4,
        "the refused insertion added nothing"
    );
}

/// Every way of naming no position is one refused mutation at exit `1`, with the position
/// named and nothing written.
///
/// **Two codes, by what the position is**: a position that is not a whole number names no
/// member of any collection, and a whole number outside the collection names no member of
/// this one. The detail says which end of the mutation the position was for, which is the
/// fact a `move` needs and a message cannot carry — human wording is not a parsing
/// contract.
#[test]
fn every_position_that_names_no_contact_is_refused_without_writing() {
    let (_tree, root) = package_with_three_contacts("contact-position-refusals");
    let before = managed_snapshot(&root);

    // Each command line is complete, so the run reaches the position under test rather
    // than a missing argument — and the last two are the destination end, which is the
    // whole reason the identity carries a role.
    for (command, code, named) in [
        (
            vec!["vendor", "contact", "edit", "Tarnwick", "0"],
            "invalid_contact_position",
            "0",
        ),
        (
            vec!["vendor", "contact", "edit", "Tarnwick", "-1"],
            "position_not_a_number",
            "-1",
        ),
        (
            vec!["vendor", "contact", "edit", "Tarnwick", "abc"],
            "position_not_a_number",
            "abc",
        ),
        (
            vec!["vendor", "contact", "edit", "Tarnwick", "4"],
            "invalid_contact_position",
            "4",
        ),
        (
            vec!["vendor", "contact", "remove", "Tarnwick", "9"],
            "invalid_contact_position",
            "9",
        ),
        (
            vec!["vendor", "contact", "move", "Tarnwick", "9", "position=1"],
            "invalid_contact_position",
            "9",
        ),
        (
            vec!["vendor", "contact", "move", "Tarnwick", "1", "position=9"],
            "invalid_contact_position",
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
            "invalid_contact_position",
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
        assert_eq!(document["error"]["code"], code, "{command:?}");
        let detail = &document["error"]["detail"];
        let quoted = if code == "position_not_a_number" {
            detail["position"].as_str().map(str::to_owned)
        } else {
            detail["position"]
                .as_u64()
                .map(|position| position.to_string())
        };
        if named != "0" {
            assert_eq!(
                quoted.as_deref(),
                Some(named),
                "{command:?}: and carries the position the command line wrote: {detail}"
            );
        }
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
    assert_eq!(document["data"]["committed"], true);
    assert_eq!(
        document["data"]["outcome"], "unchanged",
        "nothing was written, and that is a success"
    );
    assert_eq!(managed_snapshot(&root), before);
}
