# Decision traceability and contract-test conventions

Decision sources: [Chart the SpielOrders product and architecture](../../issues/wayfinder/spielorders-product-architecture/MAP.md) and [SpielOrders implementation roadmap](../../issues/roadmaps/spielorders-implementation-roadmap.md).

## Naming

Human-facing text uses decision names everywhere: a link's visible text is the governing
decision's full name, and a path or identifier may be a link target, never a substitute
for the name.

## Contract tests

Every baseline contract-test source contains a `Decision sources:` comment naming its
governing decisions.

Test and fixture names describe durable public behavior, not implementation helpers,
line numbers, or milestone and slice identifiers. One marker may govern a small source file; split a
file when its tests no longer share the same decision sources.

## Shared fixtures

`contracts/` is the one canonical contract root:

- `contracts/schemas/v1` contains durable JSON Schemas;
- `contracts/examples/v1` contains accepted minimal and full documents; and
- `contracts/fixtures` contains rejected documents, scenario recipes, and opaque
  preservation assets.

Consumers resolve these files in place rather than copying them into layer-specific test
directories, so consumers compare the same bytes. Rust tests resolve from
`CARGO_MANIFEST_DIR`; repository checks resolve from the repository root.
Swift test-resource access must also resolve this corpus without creating an independently
maintained copy.

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
own `contract_version`.

Deriving the path from the identity is enforced in
`crates/spielorders-cli/tests/response_contract.rs`: a contract cannot be filed under a
directory naming something else, the version segment is where a bump would land if there were
a consumer to break, and the fixture directory is resolved at run time from the suite's own
list of the contracts the executable emits, so one test fails for a registered contract
whose fixtures are missing. The subtree is walked in the
other direction too, in `crates/spielorders-cli/tests/cross_command_conformance.rs`: a
directory no registered contract claims is a failure, because a fixture nothing reads looks
like evidence.

A fixture holds a placeholder where a response carried a value no fixture can: the run's
`<PACKAGE_ROOT>`, `<TODAY>` for a command that reads the clock, and `<ID-n>` for each
identity, numbered in the order the response first names it, because fixture Packages are
built through the library's interface, which mints their identities; and `<DIGEST>` for an
evidence digest, which changes with the identities the evidence holds. The set is closed:
only a lowercase hyphenated UUID is numbered, only a 64-digit hexadecimal run is a
digest, and only a local date is replaced by `<TODAY>`. Every other byte is compared exactly, so a fixture diff is a contract change,
and the commit that causes it says what changed and why. No contract advances before the release
[the roadmap](../../issues/roadmaps/spielorders-implementation-roadmap.md) names: a change
edits version 1 in place and re-records its cells. Adding a
placeholder belongs to a commit that says why. For a command whose response has no
byte-stable value to record, the placeholder set is extended rather than the `success`
cell omitted.

A record a response reports names its paths absolutely. A failure or a finding names its
path the way the library's entries do: Package-relative where a Package surrounds it, and as
named where none does — the directory `init` refused because it is not a Root, or a path
the command never resolved.

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

### Supporting Material success

The [opt-in Spotlight proof](../../scripts/smoke-spotlight.sh) produces a nonempty
Supporting Material response from the shipped executable against the real index. Its
machine-readable check requires a successful command and reported files; an empty answer
or unavailable index is not a successful proof. It also moves a disposable Order to the
real Trash and takes it back out. It touches the real index and the owner's Trash, so an
owner runs it interactively, never as part of the unattended gate.

The [recorded Supporting Material answers](../../contracts/cli/search/supporting_material/v1/)
are compared automatically by the unit tests beside the
[command](../../crates/spielorders-cli/src/commands.rs), because only a live index produces
them from a run. Every cell supplies a provider's hits through the library's interface, as
the CLI's Spotlight query hands them over, and passes through the answer and envelope
`search supporting-material` builds. Only the Package Root is substituted; the sample's
other facts remain fixed and every other byte is compared. This replay does not claim to
invoke a real index or prove process-level exit and stream behavior; it keeps the recorded
response checked without depending on live index contents.

## Contradictions

See [SpielOrders agent instructions](../../AGENTS.md).

## Enforcement

Run `cargo run --package spielorders-checks --locked -- docs`. It checks local Markdown
links under `issues/`, `docs/`, and the repository root, and that governing product
decisions are closed. It does not grep source for architectural compliance. A passing
link check does not establish architectural behavior.
