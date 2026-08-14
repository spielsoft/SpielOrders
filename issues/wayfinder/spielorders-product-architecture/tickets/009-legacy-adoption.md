---
title: Define legacy Order adoption and metadata bootstrapping
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Question

How should an existing archive of physical Order Directories be adopted as a SpielOrder Package without executing migration in this planning effort: which directories are eligible, how are `config.toml` including the Orders and Removed Orders paths, `version.json`, `vendors.json`, `index.json`, per-Order `order.json` including its relative Package locator, and conventional README files proposed or created at one current package version, how are legacy institution aliases, ambiguous directory components, and duplicate Vendor evidence reconciled without Vendor Merge, and what review or dry-run evidence is required before any future write?

## Blocking

Blocked by: [Define the SpielOrders package topology](002-package-topology.md), [Define the Order record and lifecycle contract](003-order-record-lifecycle.md), [Define Vendor identity and historical semantics](004-vendor-identity-history.md), [Define the Rust CLI capability contract](006-cli-contract.md), [Define integrity, validation, and repair boundaries](008-integrity-repair.md), and [Define durable metadata schemas and version evolution](010-metadata-schemas.md). All blockers are closed.

## Resolution

Legacy adoption is an exceptional, agent-orchestrated workflow, not a general import subsystem or archive-migration engine. SpielOrders supplies only safe metadata bootstrapping primitives. An agent may inspect legacy names, structured indexes, and Supporting Material read-only; reason about contradictory evidence; prepare a consolidated proposal; and, after human approval, invoke the CLI with explicit facts. SpielOrders core never interprets Supporting Material, guesses semantic boundaries, maintains institution or Vendor alias registries, or implements an import wizard.

The workflow supports both whole-Package and individual-Order adoption, but only compositionally. Package-wide adoption means an agent initializes the Package, creates or reuses Vendors, and invokes the same per-Order primitive repeatedly. There is no separate batch importer or package-wide adoption transaction. Individual adoption remains useful for incremental work, resumption, and deliberately assigning a new identity after duplicate-Order evidence has been resolved.

### Selected archive topology and eligibility

For the contemplated `/Users/example/Documents/Administration/Orders` adoption:

- that directory is the selected **SpielOrders Root**;
- `orders_path` is `All Orders` and `removed_path` is `Removed`;
- only non-hidden, physical ordinary direct-child directories of `All Orders` are candidate Order Directories;
- the `Northwind`, `Uplands`, and `Cedarbrook` directories are institution-grouped symbolic-alias views and are excluded; links are never followed or adopted;
- `scripts`, `outputs`, `tmp`, ZIP files, `Order_Summary.xlsx`, root `README.md`, and all other root material are outside the Orders Directory and remain ordinary user space;
- hidden entries and non-directory entries under `All Orders` are excluded under the established discovery rules.

This is a selection for this archive, not a new topology mandate. Another Package may select any valid SpielOrders Root, `orders_path`, and `removed_path` allowed by the CLI and schema decisions.

### Evidence, parsing, and proposed facts

SpielOrders performs no legacy filename-to-domain parser. The aligned naming convention is a useful hint, but underscores do not reliably delimit Vendor, Order Title, or Orderer. Concrete archive names demonstrate the problem: `2005_06_01_316_Stainless_Vacuum_Chamber` has no Vendor component; `2018_02_20_Francisco_KurtJLesker` can place a person before a Vendor; `2009_04_31_Lcom_Fiber` contains an impossible calendar date; and `2019_08_22_15_12_36_Quote0161172_8_22_2019` does not expose trustworthy field boundaries. The agent may recognize a valid leading `YYYY_MM_DD` as evidence, but every adopted required fact is supplied explicitly rather than accepted from an automatic parse.

For this archive, `Order_Summary.xlsx`, the physical directory, the institution alias that points to it, retained migration audit material, and the directory's documents are separate evidence sources. The agent may inspect them read-only and should prefer direct transaction or structured-record evidence over a projected basename. It reports provenance and contradictions rather than silently selecting whichever source is easiest. Missing optional facts remain absent. Missing or contradictory required Date Ordered or Order Title blocks that candidate until the human accepts a defensible value. Status is also explicit for adoption and may be any established Order Status; it is never inferred from document presence.

The workbook's `Northwind`, `Uplands`, and `Cedarbrook` values are proposed Ordering Institution values, not directories to adopt. Reusable Free-Entry Values follow their established trimmed, case-insensitive reuse rule. An agent first queries the Package's current canonical values and recommends reuse or an explicitly confirmed new value; SpielOrders does not retain a separate legacy-alias table.

### Vendor reconciliation without Vendor Merge

The agent may discover Vendor evidence in structured records, names, and Supporting Material, but the core sees only an explicit Vendor choice or no Vendor. Before proposing a Vendor, the agent obtains the existing Vendor list and compares the evidence with current Vendor identities and facts.

- Reuse an existing Vendor only when the agent explicitly identifies it as the same real Vendor and supplies that existing association. Case and surrounding whitespace already reuse the canonical Vendor Name; broader spellings such as `Deltaparts`, `DeltaParts`, and `Delta-Parts`, or `Maplewood`, `Maplewood-Carr`, and `MaplewoodCar`, require evidence-backed agent judgment rather than automatic alias matching.
- Propose a new Vendor only when the evidence identifies a distinct Vendor not already represented. Creation uses the ordinary `add-vendor` dry-run and mutation flow.
- When no Vendor is established, omit the Vendor Reference. Never create `Unknown`, `Vendor Not Recorded`, or another placeholder Vendor.
- When two proposed names appear to be duplicates, the agent resolves them before creation by selecting one existing/new canonical Vendor. SpielOrders never creates both and later merges them; Vendor Merge remains absent.

Vendor Contacts and other shared facts are proposed only from evidence that supports Package-wide reuse. Order-specific quotes, correspondence, catalog data, and prices remain Supporting Material or Order Item facts rather than Vendor facts.

### Minimal CLI surface

The existing CLI contract gains one primitive:

```text
spielorders adopt-order DIRECTORY field=value... [--dry-run]
```

`adopt-order` is Package-rooted, like `add-order`: global `--root` identifies the exact SpielOrders Root and `DIRECTORY` selects one existing eligible direct child of its configured Orders Directory. The command requires explicit `title`, `date_ordered`, and `status`; it accepts the already defined optional Order facts and Items through the ordinary CLI mutation vocabulary. It does not parse the directory basename or inspect its contents. Vendor selection uses an already existing Vendor; creating a Vendor remains a separate `add-vendor` operation.

The existing command families cover the complete workflow:

- `init orders_path=... removed_path=... --dry-run` proposes Package bootstrapping;
- `add-vendor --dry-run` and `adopt-order --dry-run` expose each contemplated canonical mutation;
- the same commands without `--dry-run` commit approved mutations;
- `package show`, `package list vendors`, and `package values ...` expose current evidence needed for reconciliation;
- `validate` and `order validate` verify the resulting Package and Order;
- `repair`, `order repair`, and `index rebuild` provide only their already bounded recovery behavior.

There are no separate `import`, `proposal`, `review`, `resume`, or batch-adoption commands. Under `--json`, the ordinary independently versioned command contracts provide the machine-readable plans and mutation reports. Human review and the consolidated cross-Order proposal are agent artifacts, not Package metadata or persistent core state.

### Metadata bootstrapping

`init` applied to the selected unmanaged root uses only the newest metadata version understood by the running build. It creates `.spielorders/config.toml` with the explicitly selected non-default paths, `version.json` at the current version, an empty current-version `vendors.json`, and a derived current-version `index.json`. It preserves an existing root `README.md` byte-for-byte and creates an empty conventional Package README only when absent. It never writes an older schema merely to exercise the upgrade path.

Each successful `adopt-order`:

- refuses any ineligible directory, link, basename collision in selection, already managed Order, or candidate with conflicting/partial reserved `.spielorders` state;
- generates a fresh lowercase hyphenated UUIDv4 Order ID in core at commit time;
- writes only the current-version `.spielorders/order.json`, with every approved fact and a core-calculated normalized relative `package_root` locator from that Order Directory to the selected SpielOrders Root;
- preserves an existing Order `README.md` byte-for-byte and creates an empty conventional Order README only when absent;
- leaves every other file and subdirectory untouched; and
- refreshes the derived Package Index from healthy canonical state.

Vendor IDs are likewise fresh UUIDv4 values generated by the ordinary `add-vendor` core mutation. IDs are never derived from paths, names, source-system identifiers, timestamps, or content hashes. A dry run states that a fresh identity will be generated; the opaque UUID itself is not evidence requiring review.

### Conflicts, partial adoption, and safety

The agent resolves semantic duplicate-Order evidence before invoking adoption: it chooses whether evidence belongs to one Order Directory, several distinct Orders, or no useful Order. The core neither combines directories nor deduplicates Supporting Material. A physical candidate is adopted in place and is never renamed automatically; later explicit alignment remains available and retains its collision refusal.

Every newly generated Order and Vendor ID is checked against the Package before commit. An existing `order.json`, any partial prior adoption, unexpected reserved metadata, or an already adopted identity is not overwritten by adoption. Valid managed records are inspected normally; damaged or partial records go through validation and the narrowly authorized repair/recovery paths. Where the safe distinction cannot be made, the candidate remains untouched for external correction or explicit human disposition.

All mutating primitives inherit the CLI contract's complete-plan validation, beside-destination staging, atomic managed-file replacement, and rollback reporting. Initialization validates its complete root-artifact plan before writing and rolls back already-created managed artifacts where possible on failure. Each Order or Vendor commit is atomic at that entity scope. A sequence of 1,530 adoptions is deliberately not one transaction and receives no package-wide rollback: completed entities remain valid, and the agent resumes by re-reading Package state and skipping already adopted directories. A failed derived-index refresh does not undo committed canonical metadata and is recovered with `index rebuild`.

The agent must present a consolidated dry-run review before any future write. For each candidate it shows the physical relative path, proposed facts, evidence and contradictions, proposed existing/new/absent Vendor handling, reuse or introduction of Reusable Free-Entry Values, blockers, and the exact CLI mutations it intends to run. It separately summarizes exclusions, collisions, duplicate evidence, damaged or partial candidates, and expected README creation. Explicit human confirmation authorizes only that reviewed set. The CLI itself remains noninteractive: omitting `--dry-run` authorizes each named mutation, consistent with the existing contract. State-change preconditions may still reject a command between review and commit.

Supporting Material is preserved in place and byte-for-byte. Core adoption does not inventory, parse, normalize, rename, move, delete, rewrite, or create files within it. Agent inspection is read-only and creates no archive-side manifests or annotations. This planning resolution does not execute, modify, migrate, rename, or add metadata to the existing archive.

### Future SpielOrders agent skill guidance

The future agent skill owns the high-level adoption protocol. It must instruct agents to:

1. verify the exact SpielOrders Root and configured Orders and Removed Orders paths;
2. distinguish physical direct-child candidates from aliases, links, hidden entries, working directories, and other excluded material;
3. inspect legacy evidence read-only and keep source provenance, contradictions, and confidence visible;
4. treat directory components as hints rather than authoritative field boundaries;
5. query existing Vendors and Reusable Free-Entry Values before proposing reuse or creation;
6. avoid placeholder Vendors and never attempt Vendor Merge;
7. resolve duplicate Order evidence and damaged or partial prior adoption explicitly;
8. use JSON dry runs to build one consolidated human review, wait for approval, then execute only the approved commands;
9. resume from current Package state rather than replaying already committed work;
10. validate after mutations, rebuild only the derived index when indicated, and surface any rollback or recovery diagnostic; and
11. preserve all Supporting Material and never edit SpielOrders-managed metadata directly.

The exact generated skill bundle, readiness and compatibility contracts, approval policy beyond this adoption workflow, and general JSON-first operating protocol remain with [Define the SpielOrders agent skill contract](011-agent-skill-contract.md).
