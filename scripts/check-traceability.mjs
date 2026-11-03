import assert from "node:assert/strict";
import { access, readFile, readdir } from "node:fs/promises";
import path from "node:path";

async function markdown(file) {
  return readFile(file, "utf8");
}

function frontmatterValue(text, key) {
  return text.match(new RegExp(`^${key}: (.+)$`, "m"))?.[1];
}

async function markdownFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const found = [];
  for (const entry of entries) {
    const child = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name.startsWith(".")) continue;
      found.push(...(await markdownFiles(child)));
    } else if (entry.name.endsWith(".md")) {
      found.push(child);
    }
  }
  return found;
}

const linked = [...(await markdownFiles("issues")), ...(await markdownFiles("docs"))];
for (const entry of await readdir(".", { withFileTypes: true })) {
  if (entry.isFile() && entry.name.endsWith(".md")) linked.push(entry.name);
}

let checkedLinks = 0;
const brokenLinks = [];
for (const file of linked) {
  const text = await markdown(file);
  for (const [, , target] of text.matchAll(/\[([^\]]*)\]\(([^)\s]+)\)/g)) {
    if (/^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith("#")) continue;
    const relative = decodeURI(target.split("#", 1)[0]);
    if (relative === "") continue;
    checkedLinks += 1;
    try {
      await access(path.resolve(path.dirname(file), relative));
    } catch {
      brokenLinks.push(`${file}: broken link ${target}`);
    }
  }
}
assert.deepEqual(brokenLinks, [], `broken relative links:\n${brokenLinks.join("\n")}`);

// A closed list of spellings this repository has actually written, not a claim to catch every way of narrating a change.
const historyPhrases = [
  "Owner ruling",
  "Amended by the owner",
  "settled by the owner",
  "ruled by the owner",
  "used to say",
  "corrected after",
  "decomposition audit",
  "previously said",
  "superseded by",
  "Restated by the owner",
  "have since been"
];

const live = [
  ...(await markdownFiles("issues/wayfinder")),
  ...(await markdownFiles("issues/implementation")),
  ...(await markdownFiles("issues/roadmaps")),
  ...(await markdownFiles("docs")),
  "AGENTS.md",
  "CONTEXT.md"
];

const historyHits = [];
for (const file of live) {
  const lines = (await markdown(file)).split("\n");
  lines.forEach((line, index) => {
    for (const phrase of historyPhrases) {
      if (line.toLowerCase().includes(phrase.toLowerCase())) {
        historyHits.push(`${file}:${index + 1}: history phrase "${phrase}"`);
      }
    }
  });
}
assert.deepEqual(
  historyHits,
  [],
  `a live document narrates a change instead of stating the current decision:\n${historyHits.join("\n")}`
);

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

console.log(`traceability passed for ${checkedLinks} relative links and ${testFiles.length} baseline test files`);
