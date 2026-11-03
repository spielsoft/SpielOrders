---
title: Say only what the code does, wherever M5's gate found otherwise
status: open
parent: M6 — Desktop shell and read experience
labels:
  - implementation:m6
  - implementation:task
---

## Outcome

Eight claims and gaps the M5 acceptance gate found outside its own scope are each made true
or deliberately dropped, so no document, comment, or suite says something the code does not
do.

## Decision sources

- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

None.

## Scope

Eight findings, independent of one another, collected here because each is outside M5's
scope and owned by no M5 ticket.

**Documents that claim more than the code does.**

- The boundaries document says M0 supplies only interfaces and fakes. It supplies more than
  that now.
- `search supporting-material` has no recorded success response cell, while the capability
  contract says the contract directory records each command's response cells and the
  traceability guide says the placeholder set is extended rather than the success cell
  omitted. No automated run can produce one, because a real index has to answer; the
  by-hand proof script already asks the command for a machine-readable document against one.

**Values a caller can reach that mean something other than what they say.**

- A provider can report the state whose whole meaning is that this build ships no provider,
  and the command would then describe a build that has one as having none.
- A Package whose directory cannot be read is reported as a load that failed, never as a
  read the process was not permitted, while the search decision promises denied access a
  specific explanation. Package loading is not M5's, which is why no M5 ticket owned it.

**Claims nothing reaches, and code nothing calls.**

- No test lets the platform bound elapse or stops a query in flight. Every claim about a
  timeout or a cancellation is made over a reason a fake declares spent, and the adapter's
  own deadline is reached by nothing.
- The core offers a Vendor list that no production caller asks for.
- One test target builds only on unix.
- One command suite demands a complete answer wherever the build ships a provider, so it
  fails on a machine whose index is switched off, for a reason that is not about the code.
  The M5 acceptance composition carries the shape that replaces it.

## Acceptance criteria

- Each document sentence named above states what the code does, corrected by rewriting the
  sentence rather than appending to it.
- A state whose meaning is that the build ships no provider cannot be reported by a build
  that ships one.
- A Package a process may not read is told from one whose read failed for another reason,
  wherever a caller can reach the difference.
- A test lets the platform bound elapse, and a test stops a query in flight, each against
  the adapter that has to honour it.
- No test fails because the machine running it has its index switched off.
- Every response cell the contract directory promises exists, or the promise names what
  produces it.
- The Vendor list with no caller is called or gone.
- Each finding is closed by a change or recorded as deliberately dropped; none is left
  half-done.

## Exclusions

- Any advance of a contract or metadata version.
- The desktop shell this milestone otherwise builds.
