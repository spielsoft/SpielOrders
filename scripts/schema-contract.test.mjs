import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import path from "node:path";
import Ajv2020 from "ajv/dist/2020.js";

// Decision sources: Define the SpielOrders package topology; Define the Order record and lifecycle contract;
// Define Vendor identity and historical semantics; Define the Rust CLI capability contract;
// Define durable metadata schemas and version evolution.
const artifacts = ["version", "order", "vendors", "index"];
const ajv = new Ajv2020({ allErrors: true, strict: true });

async function json(file) {
  return JSON.parse(await readFile(file, "utf8"));
}

const validators = new Map();
for (const artifact of artifacts) {
  const schema = await json(`contracts/schemas/v1/${artifact}.schema.json`);
  assert.equal(schema.$schema, "https://json-schema.org/draft/2020-12/schema");
  validators.set(artifact, ajv.compile(schema));
}

for (const example of ["minimal", "full"]) {
  for (const artifact of artifacts) {
    const data = await json(path.join("contracts/examples/v1", example, `${artifact}.json`));
    const validate = validators.get(artifact);
    assert.equal(validate(data), true, `${example}/${artifact}: ${ajv.errorsText(validate.errors)}`);
  }
}

const validOrder = await json("contracts/examples/v1/minimal/order.json");
const orderValidator = validators.get("order");
assert.equal(orderValidator({ ...validOrder, package_root: ".." }), false, "single-parent package_root");
assert.equal(orderValidator({ ...validOrder, package_root: "../.." }), true, "two-parent package_root");
assert.equal(orderValidator({ ...validOrder, package_root: "../../.." }), true, "deeper package_root");
const invalidOrders = [
  ["missing package_root", ({ package_root: _, ...rest }) => rest],
  ["unknown field", (value) => ({ ...value, unknown: true })],
  ["missing items", ({ items: _, ...rest }) => rest],
  ["invalid status", (value) => ({ ...value, status: "draft" })],
  ["malformed UUID", (value) => ({ ...value, order_id: "not-a-uuid" })],
  ["lowercase currency", (value) => ({ ...value, currency: "usd" })],
  ["non-three-letter currency", (value) => ({ ...value, currency: "US" })],
  ["nonconforming date", (value) => ({ ...value, date_ordered: "07/19/2026" })],
  ["null optional", (value) => ({ ...value, vendor_id: null })]
];

for (const [name, mutate] of invalidOrders) {
  assert.equal(orderValidator(mutate(validOrder)), false, name);
}

const invalidVendor = {
  schema_version: 1,
  vendors: [{ vendor_id: "e0990d31-06aa-42e9-97c8-088e3e894822", name: "Vendor" }]
};
assert.equal(validators.get("vendors")(invalidVendor), false, "missing required contacts");

const invalidManifest = await json("contracts/fixtures/schema-invalid/manifest.json");
for (const fixture of invalidManifest) {
  const data = await json(path.join("contracts/fixtures/schema-invalid", fixture.file));
  const validate = validators.get(fixture.artifact);
  assert.equal(validate(data), false, fixture.name);
  assert.ok(validate.errors.some((error) => error.keyword === fixture.keyword), fixture.name);
}

const scenarioIndex = await json("contracts/fixtures/scenarios.json");
assert.equal(scenarioIndex.decision_sources.length, 6);
for (const source of scenarioIndex.decision_sources) {
  assert.match(source.name, /\S/);
  await access(path.join("contracts", source.path));
}

const scenarioNames = new Set();
const sourceNames = new Set(scenarioIndex.decision_sources.map((source) => source.name));
for (const scenario of scenarioIndex.scenarios) {
  assert.equal(scenarioNames.has(scenario.name), false, `duplicate scenario: ${scenario.name}`);
  scenarioNames.add(scenario.name);
  for (const field of [
    "condition",
    "schema_validity",
    "semantic_validity",
    "integrity_scope",
    "warning_state",
    "upgrade_state"
  ]) {
    assert.match(scenario[field], /\S/, `${scenario.name}: ${field}`);
  }
  assert.ok(Array.isArray(scenario.tree), `${scenario.name}: tree`);
  assert.ok(Array.isArray(scenario.preservation), `${scenario.name}: preservation`);
  assert.ok(scenario.sources.length > 0, `${scenario.name}: decision sources`);
  for (const source of scenario.sources) {
    assert.ok(sourceNames.has(source), `${scenario.name}: unknown decision source ${source}`);
  }
  for (const entry of scenario.tree) {
    if (entry.source) await access(path.join("contracts", entry.source));
  }
}

const supportingA = await readFile("contracts/fixtures/assets/supporting-a.bin");
const supportingB = await readFile("contracts/fixtures/assets/supporting-b.bin");
assert.notDeepEqual(supportingA, supportingB, "Supporting Material sentinels must differ byte-for-byte");

console.log(
  `validated 4 schemas, 8 accepted examples, ${invalidManifest.length} rejected fixtures, ` +
    `${scenarioIndex.scenarios.length} scenarios, and 2 preservation sentinels`
);
