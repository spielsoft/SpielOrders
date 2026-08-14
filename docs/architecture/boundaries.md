# Effect and adapter boundaries

Decision sources: [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../issues/wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md), [Define lookup and search behavior](../../issues/wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md), [Define the Rust CLI capability contract](../../issues/wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md), [Prototype the SpielOrders desktop experience](../../issues/wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md), and [Define durable metadata schemas and version evolution](../../issues/wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md).

The Rust core owns rules and receives effects through narrow ports. Filesystem I/O,
local calendar date, UUID generation, OS-indexed Supporting Material search, and
native open/reveal operations are effects supplied by adapters. M0 supplies only
interfaces and fakes; it does not supply production behavior.

React code consumes one `Capabilities` interface. The Tauri implementation is the
only frontend module that imports Tauri APIs. Tests use `FakeCapabilities`, so React
state and components can compile and run without Tauri. Application preferences
are a frontend capability, while ephemeral session state remains frontend-local;
neither enters the Rust core.

OS-indexed search begins asynchronously at the adapter boundary. The caller owns an
`AbortController` and must discard completion from an aborted request. The native
provider owns stopping its platform work after cancellation crosses the adapter.
The settled snapshot payload, provider states, and result algorithm are intentionally
deferred to M5.

All filesystem mechanics remain local to SpielOrders. No shared crate or frontend
package with SpielGantt is introduced.
