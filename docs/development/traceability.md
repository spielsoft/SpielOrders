# Decision traceability and contract-test conventions

Decision sources: [Chart the SpielOrders product and architecture](../../issues/wayfinder/spielorders-product-architecture/MAP.md) and [SpielOrders implementation roadmap](../../issues/roadmaps/spielorders-implementation-roadmap.md).

## Implementation tickets

Every implementation ticket contains these reviewable sections:

- `Outcome`: the user-visible or contract-level result;
- `Decision sources`: Markdown links whose visible text is the governing decision's
  full name;
- `Blocking`: native tracker dependencies or an explicit unblocked statement;
- `Scope`: work authorized by the ticket;
- `Acceptance criteria`: observable completion checks;
- `Review boundary`: the limits of review and work; and
- explicit exclusions in `Scope` or `Review boundary` when adjacent work is likely
  to be mistaken for ticket scope.

Human-facing text uses decision names. Paths and tracker identifiers may be link
targets, never substitutes for names. The milestone index links every ticket and is
updated when claims, dependencies, results, or status change.

## Contract tests

Every baseline contract-test source contains a comment in this form:

```text
Decision sources: <closed decision name>; <another closed decision name>
```

Test and fixture names describe durable public behavior, not implementation helpers,
mutable line numbers, or bare ticket identifiers. One marker may govern all tests in
one small source file. Split a file when different tests no longer share the same
decision sources.

## Shared fixtures

`contracts/` is the one canonical contract root:

- `contracts/schemas/v1` contains durable JSON Schemas;
- `contracts/examples/v1` contains accepted minimal and full documents; and
- `contracts/fixtures` contains rejected documents, scenario recipes, and opaque
  preservation assets.

Consumers resolve these files in place and do not copy them into layer-specific test
directories. Rust tests use repository-relative `include_bytes!`/`include_str!`
paths based on `CARGO_MANIFEST_DIR`; Node/TypeScript tests resolve from the repository
root or `import.meta.url`. CLI, Tauri, and frontend tests must consume the same bytes.

## Contradictions

If implementation uncovers a contradiction between closed decisions, stop the
affected behavior. Record the evidence in a new decision ticket linked by name,
block the implementation ticket on that decision, and do not settle the conflict
silently in code, fixtures, or test expectations. Continue only independent,
unblocked work that does not presume the decision.

## Enforcement

Run `npm run check:traceability`. The check reports missing M0 ticket sections,
broken or identifier-only decision links, missing milestone ticket links, and
baseline test files without recognizable closed decision names. It is a repository
check only: there is no generated traceability database, custom tracker, or runtime
dependency.

