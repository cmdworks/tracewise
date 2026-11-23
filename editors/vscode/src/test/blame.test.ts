import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { changeAtLine, labelsFor, MAX_BLAME_LINES, tooBig } from "../blame.ts";
import type { BlameRow } from "../format.ts";

const fixture = (name: string) => JSON.parse(readFileSync(new URL(`./fixtures/${name}.json`, import.meta.url), "utf8"));
const rows: BlameRow[] = fixture("blame");

test("the recorded blame has a baseline line, agent lines and a hand edit", () => {
  const kinds = rows.map((r) => r.actor.kind);
  assert.ok(kinds.includes("baseline") && kinds.includes("agent") && kinds.includes("human"), kinds.join());
});

test("one label per run, none for lines from before Tracewise", () => {
  const labels = labelsFor(rows, new Date());
  assert.ok(labels.length >= 2 && labels.length < rows.length);
  assert.ok(labels.every((l) => l.change !== "baseline"));
  const agent = labels.find((l) => l.text.startsWith("claude"));
  assert.ok(agent, "the agent's run has a label");
  assert.match(agent.text, /handle empty input without panicking/);
  assert.match(agent.hover, /\*\*claude\*\*/);
  assert.ok(agent.line >= 0, "0-based");
});

test("the label sits on the first line of its run", () => {
  const labels = labelsFor(rows, new Date());
  for (const label of labels) {
    const first = rows.find((r) => r.change === label.change)!;
    assert.equal(label.line, first.line - 1);
  }
});

test("changeAtLine finds the change under the cursor, but not a baseline line", () => {
  const agentLine = rows.find((r) => r.actor.kind === "agent")!;
  assert.equal(changeAtLine(rows, agentLine.line - 1)?.change, agentLine.change);
  const baselineLine = rows.find((r) => r.change === "baseline")!;
  assert.equal(changeAtLine(rows, baselineLine.line - 1), undefined);
  assert.equal(changeAtLine(rows, 999), undefined);
});

test("a hover without a reason says so", () => {
  const noWhy: BlameRow[] = [{ line: 1, text: "x", change: "01M7ZVPH6CYV3Z7X9D6W7CVH75", actor: { kind: "human", name: "as" }, time: "", why: null }];
  assert.match(labelsFor(noWhy, new Date())[0].hover, /no reason given/);
});

test("a huge file is not annotated", () => {
  assert.equal(tooBig(MAX_BLAME_LINES), false);
  assert.equal(tooBig(MAX_BLAME_LINES + 1), true);
});
