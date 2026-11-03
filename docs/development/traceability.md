# Decision traceability and contract-test conventions

Decision sources: [Chart the SpielOrders product and architecture](../../issues/wayfinder/spielorders-product-architecture/MAP.md) and [SpielOrders implementation roadmap](../../issues/roadmaps/spielorders-implementation-roadmap.md).

## Implementation tickets

What sections a ticket carries, open and closed, is [`AGENTS.md`](../../AGENTS.md)'s to
say, and it says it once.

What this document adds is how such a ticket names things. A `Decision sources` link's
visible text is the governing decision's full name. Human-facing text uses decision names
everywhere: paths and identifiers may be link targets, never substitutes for names. The
milestone index links every ticket and is updated when claims, dependencies, results, or
status change.

## Contract tests

Every baseline contract-test source contains a comment in this form:

```text
Decision sources: <closed decision name>; <another closed decision name>
```

Test and fixture names describe durable public behavior, not implementation helpers,
line numbers, or ticket identifiers. One marker may govern a small source file; split a
file when its tests no longer share the same decision sources.

## Shared fixtures

`contracts/` is the one canonical contract root:

- `contracts/schemas/v1` contains durable JSON Schemas;
- `contracts/examples/v1` contains accepted minimal and full documents; and
- `contracts/fixtures` contains rejected documents, scenario recipes, and opaque
  preservation assets.

Consumers resolve these files in place rather than copying them into layer-specific test
directories, so CLI, Tauri, and frontend tests consume the same bytes. Rust tests resolve
from `CARGO_MANIFEST_DIR`; Node and TypeScript tests from the repository root or
`import.meta.url`.

## CLI response golden fixtures

`contracts/cli/<contract path>/v<contract_version>/<cell>.json`, where the contract path
is the `contract` identity with its `spielorders.` namespace removed and its dots turned
into directory separators. `spielorders.package.readme` at contract version 1 is
therefore `contracts/cli/package/readme/v1/`.

A parameter in a command shape is a segment of the identity like any other, so
`package values <kind>` is four independently versioned contracts filed at
`contracts/cli/package/values/<kind>/v1/`. The `<kind>` segment is the domain's own field
name, so a category is spelled one way across the command line, the identity, the fixture
path, and the assignments the mutation commands take.

CLI response fixtures are their own subtree because the other three are the on-disk
metadata contract, versioned together by `metadata_version`, while each response carries its
own `contract_version`. An on-disk schema is never reused as a response schema, or the
reverse.

Deriving the path from the identity is enforced in
`crates/spielorders-cli/tests/response_contract.rs`: a contract cannot be filed under a
directory naming something else, the version segment is where a bump would land if there were
a consumer to break, and the fixture directory is resolved from the registry at run time, so one test
fails for a registered contract whose fixtures are missing. The subtree is walked in the
other direction too, in `crates/spielorders-cli/tests/cross_command_conformance.rs`: a
directory no registered contract claims is a failure, because a fixture nothing reads looks
like evidence.

A fixture holds a placeholder where a response carried a value no fixture can: the run's
`<PACKAGE_ROOT>`, and `<VENDOR_ID>`, `<ORDER_ID>`, or `<TODAY>` for a command that mints an
identity or reads the clock. The set is closed, and each substitution asserts the
shape of what it replaces — a lowercase hyphenated UUIDv4, or a parsable local date — before
replacing it. Every other byte is compared exactly, so a fixture diff is a contract change,
and the causing ticket says what changed and why. No contract advances before the release
[the roadmap](../../issues/roadmaps/spielorders-implementation-roadmap.md) names: a change
edits version 1 in place and re-records its cells. Adding a
placeholder belongs to a ticket that says why. For a command whose response has no
byte-stable value to record, the placeholder set is extended rather than the `success`
cell omitted.

A path a response reports carries its Package-relative spelling whenever there is a Package
around it, in a refusal exactly as in a success. `null` is for a path with no Package to be
relative to — the directory `init` refused because it is not a Root, or a path the command
never resolved.

Each cell is one response kind: `success`, `empty-result`, `warning`, `invalid`, and
`rejected` for a command that can reject a mutation. A contract supplies the cells it can
actually reach and no others. A command line that shares another's contract records no cells
of its own — the sharing is pinned behaviorally and the contract's own cells are the record.
The complete matrix across every command is
`crates/spielorders-cli/tests/cross_command_conformance.rs`: it says, contract by contract,
which cells are recorded and why the rest cannot be reached, so an absent fixture and an
impossible one do not look alike.

`usage-error` is not any command's cell. A usage error exits `2` because parsing failed, so
no command ran and none has a response to give. Every usage error carries one shared
identity, `spielorders.usage`, with its single fixture at
`contracts/cli/usage/v1/usage-error.json`, and no command may stamp one with its own.

## Contradictions

See [SpielOrders agent instructions](../../AGENTS.md).

## Enforcement

Run `npm run check:traceability`. It checks that every relative Markdown link under
`issues/`, `docs/`, and the repository root resolves, that every governing decision is
closed, and that every baseline contract-test file names a recognizable closed decision. It
is a repository check only: no generated database, tracker, or runtime dependency.
