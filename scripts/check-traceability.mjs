import assert from "node:assert/strict";
import { access, readFile, readdir } from "node:fs/promises";
import path from "node:path";

const ticketDirectory = "issues/implementation/m0-contract-baseline/tickets";
const ticketFiles = (await readdir(ticketDirectory))
  .filter((file) => /^\d{3}-.+\.md$/.test(file))
  .sort();
const requiredSections = [
  "Outcome",
  "Decision sources",
  "Blocking",
  "Scope",
  "Acceptance criteria",
  "Review boundary"
];

function frontmatterValue(text, key) {
  return text.match(new RegExp(`^${key}: (.+)$`, "m"))?.[1];
}

async function markdown(file) {
  return readFile(file, "utf8");
}

for (const file of ticketFiles) {
  const ticketPath = path.join(ticketDirectory, file);
  const text = await markdown(ticketPath);
  for (const section of requiredSections) {
    assert.match(text, new RegExp(`^## ${section}$`, "m"), `${file}: missing ${section}`);
  }

  const sourceBlock = text.match(/^## Decision sources\n([\s\S]*?)(?=^## )/m)?.[1] ?? "";
  const links = [...sourceBlock.matchAll(/\[([^\]]+)\]\(([^)]+)\)/g)];
  assert.ok(links.length > 0, `${file}: missing linked decision source`);
  for (const [, name, target] of links) {
    assert.doesNotMatch(name, /^#?\d+$/, `${file}: decision link uses a bare identifier`);
    const resolved = path.resolve(path.dirname(ticketPath), target.split("#", 1)[0]);
    await access(resolved);
  }
}

const milestone = await markdown("issues/implementation/m0-contract-baseline/README.md");
for (const file of ticketFiles) {
  assert.match(milestone, new RegExp(`\\(tickets/${file.replaceAll(".", "\\.")}\\)`), `milestone: missing ${file}`);
}

const decisionDirectory = "issues/wayfinder/spielorders-product-architecture/tickets";
const decisionNames = new Set();
for (const file of await readdir(decisionDirectory)) {
  if (!file.endsWith(".md")) continue;
  const text = await markdown(path.join(decisionDirectory, file));
  assert.equal(frontmatterValue(text, "status"), "closed", `${file}: governing decision is not closed`);
  decisionNames.add(frontmatterValue(text, "title"));
}
const map = await markdown("issues/wayfinder/spielorders-product-architecture/MAP.md");
decisionNames.add(frontmatterValue(map, "title"));

const testFiles = [
  "crates/spielorders-core/src/lib.rs",
  "crates/spielorders-core/src/ports.rs",
  "apps/desktop/src/capabilities.test.ts",
  "scripts/schema-contract.test.mjs"
];
for (const file of testFiles) {
  const text = await markdown(file);
  const marker = text.match(/Decision sources: ([\s\S]*?)(?:\n(?!\/\/)|\.\n)/)?.[1] ?? "";
  const matchedNames = [...decisionNames].filter((name) => marker.includes(name));
  assert.ok(matchedNames.length > 0, `${file}: missing a recognized closed decision name`);
}

await access("contracts/schemas/v1");
await access("contracts/examples/v1");
await access("contracts/fixtures");

console.log(`traceability passed for ${ticketFiles.length} M0 tickets and ${testFiles.length} baseline test files`);
