---
title: M3 — Domain mutations and safe persistence
status: externally validated
progress: 26 of 26 execution tickets closed, all three remediation slices verified, and the fourth finding settled across every document it named
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
  - implementation:m3
---

## Outcome

Complete the shared Rust-core write capabilities used by the future CLI and desktop
adapters: Package initialization; Order, Item, Vendor, Contact, and README mutations;
adoption, alignment, and recoverable removal; scoped diagnosis and explicit repair;
resumable Metadata Upgrade; and safe Package Index refresh.

Every mutation has a side-effect-free plan and a typed result. Canonical commits remain
distinguishable from rejection, no-op, rollback failure, and a derived-index warning
after a successful canonical write. Supporting Material remains opaque except when the
user explicitly moves or trashes a whole Order Directory.

## Current product decisions

- Vendors are hidden and unhidden, never deleted. Hidden Vendors remain resolvable but
  are not offered for new use.
- `package show` and `package list vendors` default to visible Vendors and accept
  `--all` to include Hidden Vendors.
- A managed JSON record is canonical by parsed meaning, not by byte formatting. Three
  destinations are compared by bytes instead, because in each the bytes are the subject:
  the derived Package Index, which SpielOrders owns as a cache; a Metadata Upgrade's
  transformation inputs; and metadata repair's destination, whose damaged content often
  has no parsed meaning to compare through. A capability writing several managed records
  together also compares them all by bytes, which is stricter than the rule rather than
  looser. `docs/architecture/durable-writes.md` states the full rule.
- An unreadable Order README is unknown, not empty. Index rebuild refuses to invent
  README text.
- Configured managed paths are compared by the filesystem locations they name, not by
  spelling alone.
- A mutation plan must be bound to the evidence that authorized it, and a replacement
  must refuse a destination that no longer matches the caller's observation.

The governing decision sources are:

- [Define the SpielOrders package topology](../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define the Rust CLI capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define legacy Order adoption and metadata bootstrapping](../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
- [Define durable metadata schemas and version evolution](../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Remaining execution tickets

**None. The shared worktree is free, and no execution ticket is outstanding.**

M3 failed external validation twice before passing on the third. The first failure routed
three blocking findings back to execution tickets and left a fourth as an owner decision.
All three execution slices were taken one at a time, each from a clean worktree, and the
second external validation confirmed all three genuinely remediated — each fix verified by
reverting it in an isolated clone and observing exactly one test fail:

| Slice | Ticket | Finding |
| --- | --- | --- |
| 1 | [Give single-file replacements destination evidence](tickets/020-single-file-destination-evidence.md) | V-1 |
| 2 | [Give managed record sites semantic destination expectations](tickets/022-record-round-trip-and-byte-evidence.md) and [Make whole-Order removal evidence-bound and rollback-complete](tickets/010-remove-order.md) | V-2, one code site breaking both contracts |
| 3 | [Correct semantic Order repairs](tickets/013-resolve-order-repairs.md) | V-3 |

### The fourth finding, and how it closed

The fourth finding was the owner's to settle, and the owner settled it: `orders_path = "."`
is not valid. That decision was correct and the code already matched it, but the first
settlement reached only two of the three documents the finding named and asserted in its own
commit message that it had reached all of them. That is what M3 failed on the second time.

The settlement was finished in `d0fb5ee`, which corrected
[Define durable metadata schemas and version evolution](../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
— the schema authority for `config.toml` and one of the governing decision sources listed
above — and in the same edit recorded the two refinements
[Define the Rust CLI capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
had made to that section: `config.toml` permits `removed_path` as well as `orders_path`, and
version-1 `order.json` requires the technical `package_root` locator.

[The third external validation](VALIDATION.md) checked each of those three statements for
truth rather than for presence, against the shipped code, the checked-in schemas, the
conformance fixtures, and `006`, and ran its own corpus sweep for a fourth dissenting
document rather than trusting the settlement's claim. All three hold; there is no fourth.

[Ticket 015](tickets/015-integrate-m3-acceptance.md) accepted the milestone at the gate
and its record is [ACCEPTANCE.md](ACCEPTANCE.md). That acceptance is the gate's record;
M3's standing comes from the validation.

**Nothing is outstanding on M3.**
[Ticket 027](tickets/027-hoist-package-valid-vendors-and-narrow-repair-evidence.md) is
deferred by owner decision and gates nothing, and `sha2` 0.11.0 is a new major deferred
to M4's dependency check. One non-blocking finding is recorded against `010` for the
owner's next touch of that document: its `index.json` example omits the required
`package_root`. The example's governing prose is correct, so nothing is ambiguous to an
implementer, and the block predates M2's validated target.

## Deferred, non-blocking follow-ups

Marked for later evaluation under the roadmap's rule that proven low-level duplication
need not block MVP. Neither gates M3, M4, nor any later milestone, and neither is a
defect.

| Ticket | Status |
| --- | --- |
| [Hoist the Package-valid Vendor check and narrow metadata repair's destination evidence](tickets/027-hoist-package-valid-vendors-and-narrow-repair-evidence.md) | Deferred |

## Completed foundations

The following results are complete and are not execution work:

- Package initialization, Order creation, Order adoption, Vendor visibility, Vendor
  Contacts, Order fact and Item edits, and README writes: Tickets 001–008.
- Evidence-bound Order Directory alignment: Ticket 009.
- Evidence-bound, rollback-complete whole-Order removal, including the locator rewrite's
  own failure and staging cleanup: Ticket 010.
- Repair cases bound to the evidence they diagnose, including staged-candidate content
  evidence: Ticket 011.
- Package-valid, no-clobber metadata repair, including staging-candidate promotion:
  Ticket 012.
- Correct semantic Vendor-reference, Order-identity, and physical-locator repairs,
  including the created Vendor aggregate, the nested-`orders_path` Package, and a
  generated-identity precondition that reads rather than rebuilds and distinguishes a
  collision from a Package nobody can read: Ticket 013, closing validation finding V-3.
- Chain-bound, schema-checked, resumable Metadata Upgrade planning and commit, with the
  marker advanced last on its own evidence: Ticket 014.
- The integrated milestone gate, its cross-capability composition evidence, and the
  milestone-wide review: Ticket 015.
- No-clobber replacement and plan evidence: Tickets 016, 017, and 020–023, including the
  absent-destination adoption Ticket 020 closed as validation finding V-1 and the moved
  Order record's semantic expectation Ticket 022 closed as V-2.
- Review of depended-on primitives and the resulting load/path/README corrections:
  Tickets 018 and 024–026.
- Vendor deletion replaced by visibility, including the schema and index projection:
  Ticket 019. The obsolete removal wording in Ticket 004 is superseded.

Closed ticket files are short outcome records for link stability. Git history is the
audit trail; workers should not reconstruct or repeat their former implementation
chronology.

## Execution contract

- Work one remaining ticket at a time. Record the clean starting SHA before edits.
- Use test-first implementation for each observable defect and keep the change inside
  the ticket's review boundary.
- Review each implementation ticket before closure. The final milestone review in
  Ticket 015 complements per-ticket review; it does not replace it.
- Record only current status, accepted findings, final verification, and the closing
  SHA. Do not append session transcripts, discarded designs, mutation-test diaries,
  tracker sweeps, or superseded status narratives to live tickets.
- Follow repository-level guidance for exact tools and gates instead of copying those
  instructions into every ticket.
- If implementation exposes an unresolved product decision, stop that behavior and
  route the decision explicitly. Do not settle it indirectly in a test or type.

## Handoff

A completed ticket reports its closing SHA, verification result, and the newly ready
ticket or exact blocker. The next worker loads this index, its one ticket, and only the
decision sources linked by that ticket.

There is no next execution ticket, and no next work on M3. The milestone is externally
validated at `d0fb5ee0107ab0871bcc0ee41c31d24b94c4df1b`; the next execution slice belongs
to another milestone, and the roadmap names it.
[ACCEPTANCE.md](ACCEPTANCE.md) carries the gate's candidate and closing SHAs, results, and
dispositions; [VALIDATION.md](VALIDATION.md) carries the current external validation, which
is the third. Each validation record replaces its predecessor rather than accumulating
beside it, as the Execution contract above requires of superseded narratives; the earlier
two are in Git history.
