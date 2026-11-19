# Durable managed-file writes

Decision sources: [Define the SpielOrders package topology](../../issues/wayfinder/spielorders-product-architecture/tickets/002-package-topology.md) and [Define integrity, validation, and repair boundaries](../../issues/wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md).

[The commit journal](../../crates/spielorders/src/journal.rs), the
[core filesystem primitives](../../crates/spielorders/src/filesystem.rs), and their
behavior tests define the implementation. These constraints explain the safety boundary.

## Validate where trust changes

Validate untrusted input before effects. A repair or upgrade must not introduce invalid
local or cross-record facts, but an internally typed valid plan does not need redundant
guards against unreachable states.

Reading staged output back establishes that the filesystem still holds the bytes this
operation wrote; it is not a second domain validator. Something outside SpielOrders can
change a stage before publication.

## Preserve evidence and other writers

Stage beside the destination and make the stage durable before atomic publication. A
multi-file replacement stages every file first and makes them all durable with one full
sync, which drains the device's whole cache, before publishing any of them.
A leftover stage predating the operation is evidence, not cleanup debris. Promotion
retains destination preconditions and durability without destroying that evidence on failure.

Choose changed-destination evidence according to the subject: parsed meaning for
single-record semantic edits, exact bytes where text or damaged content is the subject,
and observed absence only when absence was actually established. An unreadable existing
destination is not absent. Comparison uses the same normalized meaning planning used.

A commit undoes its completed steps in reverse order: a created entry only while change
evidence proves it is the one the operation created, and a replaced file only while it
holds exactly the bytes the operation wrote. A moved entry goes back by a no-replace move,
which needs only its old name to be free: that move destroys nothing whatever stands at
the new name, so it asks no proof of ownership. An undo never guesses. On a volume that keeps no comparable change evidence,
nothing proves a created entry is the operation's own, so the undo removes or restores
nothing it cannot prove it made and names each entry it left behind instead. Preserve a
later writer's work; what the undo could not do keeps its location and remains visible
beside the initiating error.

Atomicity is per file. Handled failures attempt rollback, but process or power interruption
can leave part of a multi-file operation published.

## Planning reads; commit writes

Precondition checks and dry runs do not rebuild indexes or create artifacts. A refusal
to attempt a mutation writes nothing. A failed mutation can have attempted effects and
rollback failures; do not collapse those different outcomes.

Canonical commit and derived-index refresh are different results. If canonical data
committed and refresh failed, report a committed warning, not a rejection inviting retry.
Steps after a commit's last publishing step only tidy up; their failure is a committed
warning.

A cache cannot invent missing evidence. If a required README exists but its text is
unavailable, do not replace that fact with an empty cached document. Missing README and
unreadable README are distinct.

## Directory effects

Directory publication needs one no-replace filesystem operation. A separate existence
check followed by ordinary replacing rename can destroy a directory created between
the two calls. Lazy directory creation must likewise refuse an existing entry, or
rollback can mistake a user's directory for its own.

These primitives are not filesystem locking or compare-and-swap. Revalidate immediately
before effects, preserve concurrent work where detected, and report the remaining limits.
Whole-directory creation and disposition are their own steps in a commit — a no-replace
directory creation and a no-replace move — never file replacements; their undo follows
the same ownership rule.
No file-write abstraction grants authority to inspect Supporting Material.
