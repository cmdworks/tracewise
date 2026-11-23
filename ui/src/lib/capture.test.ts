import { test } from "node:test";
import assert from "node:assert/strict";
import { captureHeadline, captureLines, parseCapture } from "./capture.ts";

const sample = {
  preset: "balanced",
  layers: { terminal: true, daemon: false, watcher_running: false, hooks: ["claude"] },
  mode: "agents",
  problems: [],
  warnings: []
};

test("the answer of tracewise capture is read, and anything else is not", () => {
  const c = parseCapture(sample);
  assert.equal(c?.preset, "balanced");
  assert.deepEqual(c?.layers.hooks, ["claude"]);
  assert.equal(parseCapture(null), null);
  assert.equal(parseCapture({ preset: "x" }), null);
  assert.equal(parseCapture({ ...sample, preset: null })?.preset, null);
  assert.deepEqual(parseCapture({ ...sample, problems: ["a", 3] })?.problems, ["a"]);
});

test("each layer says whether it is on and what that means", () => {
  const lines = captureLines(parseCapture(sample)!);
  assert.deepEqual(lines.map((l) => [l.name, l.on]), [["Agent hooks", true], ["Terminal", true], ["Watcher", false]]);
  const off = captureLines(parseCapture({ ...sample, layers: { terminal: false, daemon: true, watcher_running: false, hooks: [] } })!);
  assert.match(off[0].text, /none installed/);
  assert.match(off[2].text, /not running/);
  const running = captureLines(parseCapture({ ...sample, layers: { ...sample.layers, daemon: true, watcher_running: true } })!);
  assert.match(running[2].text, /^running/);
});

test("the headline names the preset or says there is none", () => {
  assert.equal(captureHeadline(parseCapture(sample)), "Capture: balanced");
  assert.match(captureHeadline(parseCapture({ ...sample, preset: null })), /custom/);
  assert.match(captureHeadline(null), /not loaded/);
});
