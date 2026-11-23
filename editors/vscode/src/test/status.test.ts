import { test } from "node:test";
import assert from "node:assert/strict";
import { viewOf } from "../status.ts";

test("a tracked project shows the pending count and opens the pending list", () => {
  const view = viewOf({ kind: "tracked", mode: "agents", pending: 3 });
  assert.match(view.text, /3 without a reason/);
  assert.equal(view.command, "tracewise.showPending");
  assert.match(view.tooltip, /agents/);
});

test("nothing pending is quiet", () => {
  const view = viewOf({ kind: "tracked", mode: "full", pending: 0 });
  assert.equal(view.text, "$(history) Tracewise");
  assert.equal(view.command, "tracewise.refresh");
});

test("a folder that is not a project is quiet: no warning icon", () => {
  const view = viewOf({ kind: "not-tracked" });
  assert.ok(!view.text.includes("$(warning)"));
  assert.equal(view.command, undefined);
});

test("a missing or old program says what to do", () => {
  assert.match(viewOf({ kind: "missing" }).tooltip, /Install it, or set tracewise.path/);
  assert.match(viewOf({ kind: "old" }).tooltip, /Update it/);
  assert.match(viewOf({ kind: "error", message: "boom" }).tooltip, /boom/);
});

test("an untrusted folder says why nothing happens and offers to manage trust", () => {
  const view = viewOf({ kind: "untrusted" });
  assert.match(view.tooltip, /trust/);
  assert.equal(view.command, "workbench.trust.manage");
});

test("a newer program asks for an extension update, not a tracewise update", () => {
  assert.match(viewOf({ kind: "newer" }).tooltip, /Update the extension/);
});
