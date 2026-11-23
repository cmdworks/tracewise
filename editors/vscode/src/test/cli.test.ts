import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { check, forgetPrograms, run } from "../cli.ts";

beforeEach(() => forgetPrograms());

/** A stand-in `tracewise`: a shell script that answers by its arguments, and records how it was called. */
function fake(script: string): { program: string; log: string } {
  const dir = mkdtempSync(join(tmpdir(), "tw-fake-"));
  const program = join(dir, "tracewise");
  const log = join(dir, "calls.log");
  writeFileSync(program, `#!/bin/sh\necho "$@" >> "${log}"\n${script}\n`);
  chmodSync(program, 0o755);
  return { program, log };
}

const opts = (program: string, timeoutMs = 3000) => ({ program, cwd: tmpdir(), timeoutMs });

test("run asks for JSON, from the file's folder, and parses the answer", async () => {
  const { program, log } = fake(`echo '[{"line":1}]'`);
  const r = await run(["blame", "/p/a.ts"], opts(program));
  assert.deepEqual(r, { ok: true, json: [{ line: 1 }] });
  const { readFileSync } = await import("node:fs");
  assert.equal(readFileSync(log, "utf8").trim(), `-C ${tmpdir()} blame /p/a.ts --json`);
});

test("exit code 2 is 'not a project', quietly", async () => {
  const { program } = fake(`echo "tracewise: not a Tracewise project" >&2; exit 2`);
  const r = await run(["status"], opts(program));
  assert.equal(r.ok, false);
  assert.equal(!r.ok && r.reason, "not-a-project");
});

test("another failure carries the program's message", async () => {
  const { program } = fake(`echo "disk is full" >&2; exit 1`);
  const r = await run(["record", "a.ts"], opts(program));
  assert.equal(!r.ok && r.reason, "failed");
  assert.match(!r.ok ? r.message : "", /disk is full/);
});

test("a program that is not there is 'missing'", async () => {
  const r = await run(["status"], opts("/no/such/tracewise"));
  assert.equal(!r.ok && r.reason, "missing");
});

test("a program that takes too long is 'timeout'", async () => {
  const { program } = fake(`sleep 5`);
  const r = await run(["status"], opts(program, 150));
  assert.equal(!r.ok && r.reason, "timeout");
});

test("output that is not JSON is a failure, not a crash", async () => {
  const { program } = fake(`echo "hello"`);
  const r = await run(["status"], opts(program));
  assert.equal(!r.ok && r.reason, "failed");
});

test("only the commands the extension needs can be run", async () => {
  const { program, log } = fake(`echo '{}'`);
  for (const bad of [["config", "set", "x", "y"], ["agent", "install", "claude"], ["hook"], ["uninstall"], [], ["--help"]]) {
    const r = await run(bad, opts(program));
    assert.equal(!r.ok && r.reason, "not-allowed", JSON.stringify(bad));
  }
  const { existsSync } = await import("node:fs");
  assert.equal(existsSync(log), false, "nothing was started");
});

test("a git repository that was never initialised is 'not a project', quietly", async () => {
  const { program } = fake(`echo "tracewise: this repository has no Tracewise history yet; run tracewise init first" >&2; exit 1`);
  const r = await run(["blame", "/p/a.ts"], opts(program));
  assert.equal(!r.ok && r.reason, "not-a-project");
});

test("a folder that is gone is not reported as a missing program", async () => {
  const { program } = fake(`echo '{}'`);
  const r = await run(["status"], { program, cwd: "/no/such/folder", timeoutMs: 1000 });
  assert.equal(!r.ok && r.reason, "failed");
  assert.match(!r.ok ? r.message : "", /does not exist/);
});

test("check: a program with the features is ok, an old one is old, a missing one is missing", async () => {
  const modern = fake(`echo '{"api":1,"features":["json-contract","record"]}'`);
  assert.equal(await check(modern.program, ["json-contract", "record"]), "ok");
  const withoutRecord = fake(`echo '{"api":1,"features":["json-contract"]}'`);
  assert.equal(await check(withoutRecord.program, ["record"]), "old");
  const ancient = fake(`echo '{"api":1}'`);
  assert.equal(await check(ancient.program, ["json-contract"]), "old");
  const broken = fake(`exit 3`);
  assert.equal(await check(broken.program, ["json-contract"]), "old");
  assert.equal(await check("/no/such/tracewise", ["record"]), "missing");
});

test("the program is asked about its features once per session", async () => {
  const { program, log } = fake(`echo '{"features":["json-contract"]}'`);
  await check(program, ["json-contract"]);
  await check(program, ["json-contract"]);
  const { readFileSync } = await import("node:fs");
  assert.equal(readFileSync(log, "utf8").trim().split("\n").length, 1);
});

test("check: a failure is not remembered, so installing or updating takes effect without a reload", async () => {
  const dir = mkdtempSync(join(tmpdir(), "tw-late-"));
  const program = join(dir, "tracewise");
  assert.equal(await check(program, ["json-contract"]), "missing");
  writeFileSync(program, `#!/bin/sh\necho '{"api":1,"features":["json-contract"]}'\n`);
  chmodSync(program, 0o755);
  assert.equal(await check(program, ["json-contract"]), "ok");
});

test("check: a program with a newer api than the extension knows is 'newer'", async () => {
  const future = fake(`echo '{"api":2,"features":["json-contract","record"]}'`);
  assert.equal(await check(future.program, ["json-contract"]), "newer");
});
