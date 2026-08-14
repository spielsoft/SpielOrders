import assert from "node:assert/strict";
import test from "node:test";
import { SessionState } from "./capabilities.ts";
import { FakeCapabilities } from "./testing/fake-capabilities.ts";

// Decision sources: Define lookup and search behavior; Define the Rust CLI capability contract;
// Prototype the SpielOrders desktop experience; Define durable metadata schemas and version evolution.

test("React-facing code can substitute fake capabilities without Tauri", async () => {
  const capabilities = new FakeCapabilities();
  const cancellation = new AbortController();

  await capabilities.preferences.write("density", "compact");
  await capabilities.nativeFiles.open("/fixture/order.pdf");
  await capabilities.nativeFiles.reveal("/fixture/order.pdf");
  await capabilities.supportingMaterialSearch.search("invoice", cancellation.signal);

  assert.equal(await capabilities.preferences.read("density"), "compact");
  assert.deepEqual(capabilities.opened, ["/fixture/order.pdf"]);
  assert.deepEqual(capabilities.revealed, ["/fixture/order.pdf"]);
  assert.deepEqual(capabilities.searches, ["invoice"]);
});

test("session state remains frontend-local", () => {
  const session = new SessionState();
  session.write("selection", "order-1");
  assert.equal(session.read("selection"), "order-1");
});
