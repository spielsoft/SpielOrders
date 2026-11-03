# SpielOrders agent instructions

## Dependency currency

- Keep every toolchain, runtime, and library on the newest minor and patch release
  within its selected major. Apply those updates without separate approval.
- Ask the owner before adopting a new major version.
- Never fall back silently to not updating. If an in-major update is blocked, report the
  blocker and the newest version you could verify.

## Reading rule

A worker loads this file, the milestone index's ticket table, its one ticket, and the
decision documents that ticket links. Nothing else is read by default. Closed tickets,
archived milestones, acceptance and validation records, and other milestones are history;
open one only when your ticket links it by name.

## Two lanes

**Small change.** The diff changes at most 50 lines of non-test source, no file under
`contracts/`, no `contract_version`, no exit status, no public `spielorders-core`
signature, and no `metadata_version`. It gets one commit containing the change and its
test, one `scripts/gate.sh` run, and a commit message whose first body line is
`Small change: <one clause saying why it qualifies>`. No ticket, no claim, no review. A
defect fix, a rendering fix, a missing test, a doc correction, and a dependency patch
bump are the expected cases.

**Ticket.** Everything else.

`AGENTS.md` itself is outside both lanes. It changes only on a ruling from the owner or
the peer session, and the commit body names that ruling by date so the provenance is
checkable.

## Ticket execution

One ticket is in progress at a time across all milestones, because there is one worktree.

Claim: with a clean tree, record `git rev-parse HEAD` as `review_start`, set
`status: in progress`, `assignee`, and `claimed_at`, and commit the claim alone.

Build with the `implement` and `tdd` skills, through the executable's own surface. The
core owns Package rules; the CLI parses, targets, invokes, presents, exits. Human output
is never a parsing contract. Commit each coherent step: uncommitted work is the only work
that is ever lost. Run `scripts/gate.sh` once when the implementation is stable and once
after review remediation.

"Report it and stop" in a ticket or a prompt means stop that line of work and record what
stopped it in the ticket; a ticket halts only when an acceptance criterion cannot be met
or a product decision is missing.

## Review

Every ticket gets the project `code-review` skill in its two-axis parallel form: two
sub-agents spawned in one message, run in the foreground, each appending its findings to
a file as it reads. Do not end your turn while either is outstanding.

A sub-agent never mutates the tree; it reports the mutation it wants performed.

Blocking findings are fixed. A non-blocking finding is fixed if it takes under five
minutes and is otherwise dropped, not recorded.

If the parallel mechanism is unavailable, stop and report. Never substitute one reader.

## Records

- One home per fact. A rule lives in the code or in exactly one document. Everywhere else
  links to it by name and never restates it.
- A document states the current decision in the present tense. No dates, amendments,
  rulings, or "previously" in a live document; when a ruling changes a document, the
  sentence it governs is rewritten. The commit body names the ruling.
- A decision document describes what is not yet built. Once a surface is built and
  accepted, the document keeps the rules that constrain future change and points at the
  code and fixtures for what exists.
- No counts, SHAs, test totals, file inventories, or version numbers in prose. The
  exceptions are ticket frontmatter (`review_start`, `closed_at`) and a milestone's
  acceptance record.
- A closed ticket is immutable. Its `## Result` is at most 150 words: what shipped, where
  the tests are, and links to any follow-up tickets. No review tables, no inventories, no
  verification transcripts.
- An open ticket has the sections Outcome, Decision sources, Blocking, Scope, Acceptance
  criteria, Exclusions, and at most 600 words outside its acceptance criteria. No
  predicted code shapes, no file paths, no fixture matrices. Implementation discovers the
  shape.
- A milestone index is the ticket table plus scope and acceptance gate. It carries no
  narrative about what closed tickets found.
- A code comment may name a decision. It does not restate it.
- Structural tests that grep the source tree are not added. Architecture is enforced by
  visibility (`pub(crate)`), by types, and by clippy.
- A decision decides; code chooses. A fence in code enforces a sentence a decision states.
  A choice the code made is a comment that says it is a choice, never a rule, and never a
  fence.

## Contradictions

If a document is wrong and the correct behavior is unambiguous, fix the document in the
same commit and say so in the message.

If two closed decisions genuinely conflict on a product question, stop the affected
behavior and ask the owner, one question at a time, naming both documents. Do not open a
ticket to ask.

## Handoff

A finished ticket reports its closing SHA, the gate result, and the next eligible ticket.
Nothing else.

## The machine is the owner's laptop

Do not generate synthetic load. Release background jobs from a `trap`, never on a final
line.
