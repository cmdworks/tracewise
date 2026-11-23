import { test } from "node:test";
import assert from "node:assert/strict";
import { actorLabel, blameLabel, groupRuns, isChangeId, relativeTime, type BlameRow } from "../format.ts";

const now = new Date("2026-11-22T12:00:00Z");
const ago = (ms: number) => new Date(now.getTime() - ms).toISOString();
const row = (line: number, change: string, over: Partial<BlameRow> = {}): BlameRow => ({
  line,
  text: "x",
  change,
  actor: { kind: "agent", name: "claude", session: "s1" },
  time: ago(2 * 3600_000),
  why: null,
  ...over,
});

test("relativeTime steps through minutes, hours, days, weeks", () => {
  assert.equal(relativeTime(ago(10_000), now), "just now");
  assert.equal(relativeTime(ago(5 * 60_000), now), "5m ago");
  assert.equal(relativeTime(ago(3 * 3600_000), now), "3h ago");
  assert.equal(relativeTime(ago(3 * 86_400_000), now), "3d ago");
  assert.equal(relativeTime(ago(21 * 86_400_000), now), "3w ago");
  assert.equal(relativeTime(ago(400 * 86_400_000), now), "13mo ago");
  assert.equal(relativeTime("not a time", now), "");
  assert.equal(relativeTime(ago(-5000), now), "just now", "a time in the future (clock skew) is not negative");
});

test("actorLabel says something for every kind of actor", () => {
  assert.equal(actorLabel({ kind: "human", name: "as" }), "as");
  assert.equal(actorLabel({ kind: "human" }), "someone");
  assert.equal(actorLabel({ kind: "agent", name: "claude" }), "claude");
  assert.equal(actorLabel({ kind: "baseline" }), "before Tracewise");
  assert.equal(actorLabel({ kind: "ambiguous" }), "unclear who");
  assert.equal(actorLabel({ kind: "unknown" }), "unknown");
  assert.equal(actorLabel({ kind: "something-new" }), "unknown");
});

test("blameLabel joins who, when and why", () => {
  assert.equal(blameLabel(row(1, "A", { why: "handle empty input" }), now), "claude · 2h ago · handle empty input");
  assert.equal(blameLabel(row(1, "A"), now), "claude · 2h ago");
  assert.equal(blameLabel(row(1, "baseline", { actor: { kind: "baseline" }, time: "", git_author: "Pat" }), now), "before Tracewise · Pat");
});

test("groupRuns makes one run per change, and a change that wrote two stretches is two runs", () => {
  const rows = [row(1, "A"), row(2, "A"), row(3, "B"), row(4, "A"), row(5, "A"), row(6, "A")];
  const runs = groupRuns(rows).map((r) => [r.row.change, r.start, r.end]);
  assert.deepEqual(runs, [["A", 1, 2], ["B", 3, 3], ["A", 4, 6]]);
  assert.deepEqual(groupRuns([]), []);
});

test("only a 26-character id is a change id", () => {
  assert.equal(isChangeId("01M7ZVPH6CYV3Z7X9D6W7CVH75"), true);
  assert.equal(isChangeId("baseline"), false);
  assert.equal(isChangeId("01M7ZVPH6CYV3Z7X9D6W7CVH75; rm -rf /"), false);
  assert.equal(isChangeId("--help"), false);
});
