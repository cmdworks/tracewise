import { test } from "node:test";
import assert from "node:assert/strict";
import { debouncedPerKey, pathToRecord, type Timers } from "../save.ts";

test("a file inside a workspace folder is recorded, one outside is not", () => {
  const folders = ["/work/app", "/work/lib"];
  assert.equal(pathToRecord("/work/app/src/a.ts", folders, true), "/work/app/src/a.ts");
  assert.equal(pathToRecord("/work/lib/b.ts", folders, true), "/work/lib/b.ts");
  assert.equal(pathToRecord("/work/other/c.ts", folders, true), undefined);
  assert.equal(pathToRecord("/work/app/../other/c.ts", folders, true), undefined, "a path that climbs out is outside");
  assert.equal(pathToRecord("/work/app", folders, true), undefined, "the folder itself is not a file");
});

test("the setting turns recording off", () => {
  assert.equal(pathToRecord("/work/app/a.ts", ["/work/app"], false), undefined);
});

/** A clock the test moves by hand. */
function fakeTimers() {
  let now = 0;
  const pending = new Map<number, { at: number; fn: () => void }>();
  let next = 1;
  const timers: Timers = {
    set: (fn, ms) => {
      pending.set(next, { at: now + ms, fn });
      return next++;
    },
    clear: (h) => void pending.delete(h as number),
  };
  const advance = (ms: number) => {
    now += ms;
    for (const [id, t] of [...pending]) if (t.at <= now) (pending.delete(id), t.fn());
  };
  return { timers, advance };
}

test("three saves of one file in a burst make one call, after the last", () => {
  const { timers, advance } = fakeTimers();
  const calls: string[] = [];
  const save = debouncedPerKey(300, (k) => calls.push(k), timers);
  save("a.ts");
  advance(100);
  save("a.ts");
  advance(100);
  save("a.ts");
  advance(299);
  assert.deepEqual(calls, []);
  advance(1);
  assert.deepEqual(calls, ["a.ts"]);
});

test("different files do not delay each other", () => {
  const { timers, advance } = fakeTimers();
  const calls: string[] = [];
  const save = debouncedPerKey(300, (k) => calls.push(k), timers);
  save("a.ts");
  advance(200);
  save("b.ts");
  advance(100);
  assert.deepEqual(calls, ["a.ts"]);
  advance(200);
  assert.deepEqual(calls, ["a.ts", "b.ts"]);
});
