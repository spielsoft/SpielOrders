# Shared filesystem-bucket foundation

## Decision

**Defer extraction, while giving SpielOrders compatible boundaries from the start.**

SpielOrders should not depend on SpielGantt or extract a shared crate before its own package topology and lifecycle are settled. It should implement its own order-domain core behind a narrow filesystem repository boundary. Once both applications have exercised the same filesystem invariants, extract only the common mechanics into a small Rust crate used by both.

The eventual shared responsibility should be named **filesystem bucket mechanics**, not `FilesystemDatabase`. The latter implies query, indexing, transactions, and relational semantics that should remain application-specific.

## What SpielGantt actually does

SpielGantt already has the correct high-level separation for SpielOrders to emulate:

- `src-tauri/src/cli/*` is a thin CLI adapter over public Rust behavior.
- `src-tauri/src/app_facade.rs` exposes application operations shared by CLI-capable behavior and the GUI.
- `src-tauri/src/tauri_commands.rs` is a thin Tauri transport adapter.
- `src/shell-tauri-adapter.ts` translates frontend calls to Tauri commands, while `src/shell-types.ts` defines frontend capability interfaces.
- React owns desktop/session/presentation concerns; Rust owns package semantics. SpielGantt's project guidance explicitly requires the frontend to be replaceable by CLI JSON calls plus local rendering and GUI/session code.

Its filesystem-bucket implementation is real but not presently a reusable subsystem:

- Project-root detection and metadata initialization are embedded in the `project` module inside `src-tauri/src/lib.rs`, with `.spielgantt/project.json` hard-coded.
- Direct-child discovery is isolated in `task_package_discovery.rs`, but it returns SpielGantt graph errors and hard-codes `.spielgantt/task.json` plus a SpielGantt-specific staged-delete prefix.
- `task_package_index.rs` combines filesystem reads with `TaskMetadata`, project graph construction, semantic facts, strict/diagnostic policy, and Gantt-specific identity checks.
- `package_mutation.rs` contains potentially reusable rewrite/rollback mechanics, but its API is expressed in terms of `MetadataRewrite`, JSON filenames, and SpielGantt cleanup behavior.
- README reading and content-version checks live in `project_payload.rs`, `project_actions.rs`, and task edit modules rather than a shared text-document abstraction.
- Creation, adoption, normalization, folder alignment, and deletion are intertwined with task IDs, dependencies, events, and graph rewrites.
- The crate is a single Tauri package (`spielgantt_lib`), not a Cargo workspace containing a proven foundation crate.

The frontend has a useful architectural pattern but no coherent shared order/task model. Its payloads, capabilities, sidebar behavior, inspector state, and timeline models are intentionally Gantt-specific. Visual similarity between the apps does not justify coupling their frontend domain code.

## Why extraction now is premature

SpielOrders has not yet fixed several policies that would become accidental shared abstractions if extracted now: root metadata layout, the optional resources area, the exact title-to-folder invariant, vendor storage, search boundaries, and order lifecycle mutation rules. Meanwhile, the generic-looking SpielGantt code still carries application-specific paths, errors, domain objects, and cleanup assumptions.

Extracting now would therefore require either:

1. making SpielOrders conform to SpielGantt decisions before SpielOrders has made its own decisions, or
2. designing a broad configurable framework around only one existing implementation.

Both choices create more coupling than reuse. Remaining permanently independent is also unnecessary: the apps clearly share a small filesystem substrate, and compatible boundaries preserve a low-cost future extraction path.

## Compatible boundary for SpielOrders

SpielOrders should keep these layers distinct:

1. **Order domain** — Order, Order Item, Vendor reference/snapshot, status, institution, Project, Order Group, validation, and lifecycle rules.
2. **Order package repository** — root discovery, Order Directory discovery, loading, diagnostics, and mutations expressed in Order terms.
3. **Filesystem mechanics** — path safety, direct-child enumeration, managed-marker recognition, preservation of opaque user files, safe file replacement, and safe bucket directory operations.
4. **Adapters** — CLI JSON/human output, thin Tauri commands, and desktop-only services such as Finder/Spotlight integration.

The repository should depend inward on the Order domain and downward on a small filesystem-mechanics interface. Domain metadata types must not implement raw traversal or file replacement themselves. CLI and Tauri code must call the same repository/application operations.

SpielGantt can later be moved behind an equivalent boundary without changing its public CLI or Tauri contracts.

## Narrowest coherent future shared crate

A future crate—provisionally `filesystem-buckets`—should contain only policy-neutral mechanics:

- validate a user-visible identity as one portable filesystem path component;
- enumerate ordinary direct-child directories and recognize a bucket from a caller-supplied managed marker path;
- represent managed paths without claiming ownership of other bucket contents;
- safely create or adopt a bucket while preserving existing user files;
- safely rename a bucket with collision/preflight reporting;
- replace managed text/JSON files using tested temporary-file and rollback behavior;
- read and conditionally write a managed or conventional text file using an expected content version.

The caller supplies metadata filenames and performs serialization, schema validation, identity rules, cross-record validation, and domain mutation planning. The shared crate should not contain:

- Order, Vendor, Task, event, dependency, lifecycle, graph, or search semantics;
- a universal project/bucket JSON schema;
- CLI commands, Tauri commands, frontend types, sidebar models, or GUI components;
- Spotlight integration or indexing;
- destructive whole-project deletion policy;
- cache/database/query abstractions.

This is intentionally smaller than a `FolderBuckets` framework. A shared crate should make dangerous filesystem operations consistent without becoming the source of either application's domain model.

## Extraction trigger and sequence

Do not create the shared crate during the initial SpielOrders architecture or scaffold. Extract after all of the following are true:

- SpielOrders has a tested package topology and at least create, load, adopt/import, edit, rename, and validation flows.
- The same operation has behavior tests in both applications.
- The invariants match without application-name conditionals or domain callbacks that contain most of the behavior.
- Cross-platform replacement, collision, symlink, partial-failure, and rollback expectations are explicit.

At that point, extract one operation family at a time, starting with portable path-component validation and direct-child marker discovery. Move mutation mechanics only after their durability contract is strengthened and agreed by both apps. Keep the crate independently versioned or in a deliberately managed shared Cargo workspace/repository; do not make either application depend on the other's app crate.

## Consequences for the remaining map

- The SpielOrders package topology may be designed on its own merits, while preserving direct-child bucket and opaque-user-file compatibility where useful.
- The Rust CLI/core/Tauri boundary should follow SpielGantt's proven shape.
- Shared UI look-and-feel is a separate concern from filesystem sharing and should not be bundled into this foundation decision.
- A future extraction is an implementation refactor, not a prerequisite for specifying SpielOrders.
