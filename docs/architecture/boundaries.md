# Effect and adapter boundaries

Decision sources: [Define the Rust CLI capability contract](../../issues/wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md), [Prototype the SpielOrders desktop experience](../../issues/wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md), and [Define durable metadata schemas and version evolution](../../issues/wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md).

## Rust ownership

The [library](../../crates/spielorders/src/lib.rs) exposes only its C interface. It owns
Package reads, mutation planning, validation, identity, normalization, calculations,
search processing, diagnostics, and recovery. It answers every monetary value unrounded;
each front end chooses the precision it displays.
[Integration ownership](#integration-ownership) governs coordination with Rust work.

The library performs its own Package filesystem effects inside the authorized folder.
It performs no OS-native service: Spotlight, Trash, and
Finder belong to each front end, the desktop in Swift and the CLI in
[its own platform module](../../crates/spielorders-cli/src/platform.rs). Core depends on
neither an adapter nor a GUI toolkit.

The CLI parses and targets, calls the interface, builds its own JSON from what the
interface answers, presents, and exits. Human output is not a
parsing contract. A frontend replacement alone requires no metadata change.

## Swift desktop boundary

The [Swift desktop foundation](../../apps/macos/App/SpielOrdersApp.swift) follows
[Native desktop distribution](../adr/0001-native-desktop-distribution.md) and directly
links Rust in the app process; the GUI does not use CLI subprocesses or a backend
service as its domain transport.

The core has one public interface, written in Rust over the C ABI, that every calling
language uses the same way: Swift, C, Fortran, Python `ctypes`, and the Rust CLI. One C
header, [`spielorders.h`](../../crates/spielorders/include/spielorders.h),
generated from [the interface module](../../crates/spielorders/src/interface/mod.rs),
documents all of it. Swift imports that header and links the static library as described
in [Development](../../README.md#development). There are no generated bindings,
language-specific second interfaces, serialized request or response documents standing
in for named functions, or callbacks for library-owned effects.

### Interface rules

The generated header is authoritative for arguments, nullability, handles, borrowed
strings and paths, constants, results, ownership, panics, drafts, and cancellation.
Do not edit it by hand. The [C Package walkthrough](../../crates/spielorders/tests/c/package_walkthrough.c)
shows a complete caller; the repository gate checks header freshness and runs that
walkthrough against the linked library. Swift adapters follow that contract rather than
copying a second specification into frontend documentation.

Swift owns presentation, focus, selection, editor sessions, undo, window/session state,
and app-owned preferences. The library's Draft holds editable domain input; Swift binds
controls to it without rejecting or normalizing the user's text independently.
UI state stays outside Package metadata. Ordinary validation,
status/date behavior, reusable-value matching, Vendor resolution, numeric parsing,
and totals remain Rust responsibilities. Swift may manage input text, layout, and the
display precision of money, not independently calculate the answer.

Editors pass original-read evidence through to Rust planning and commit. A Swift-only
comparison is not changed-document protection. Mutation presentation distinguishes
rejection from a committed mutation carrying warnings.

Long-running Rust calls execute off the UI thread. The desktop carries cancellation,
rejects stale responses, and settles owned work before exit. Security-scoped folder
access stays active until all library work using it settles, including commits.

### Native services

Swift runs the Spotlight query over the directory the library names until gathering
completes or the user stops it, without an application-imposed time limit. It supplies
the settled hits, provider state, and diagnostics through the
header's Hits operations and Supporting Material search. The library alone decides
eligibility, grouping, and ordering. Native API use alone does not prove provider-state
or stopping semantics; no async runtime enters the library.

Swift reveals directories and matched files in Finder. Moving an Order to Trash first
commits the library's recoverable removal, then uses Swift's native Trash service on the
final directory the report names. A failed Trash move leaves that Order recoverable;
Swift does not roll back or retry the committed removal as though it had failed.

The expert raw-JSON exception is defined only by the desktop decision. All ordinary
Package operations use Rust capabilities.

## Integration ownership

Desktop integration adds no Rust interface or crate and does not edit existing Rust
crates or the generated header. A needed library capability is requested through the
owner for the Rust worker rather than added incidentally by the desktop worker.

Production Swift integration consumes the header, so its callable signatures are
additive-only; breaking changes require owner approval. Internals the header does not
expose remain free to change.

## Reuse boundary

SpielOrders does not depend on SpielGantt or create a shared frontend framework.
Extract filesystem mechanics only when both products demonstrate matching invariants.
