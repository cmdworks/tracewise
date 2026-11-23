import { execFile } from "node:child_process";
import { existsSync } from "node:fs";

/** The `api` of `tracewise companion` this extension was written for. A bigger one may have changed a shape. */
export const KNOWN_API = 1;

/** The only commands the extension runs. Nothing from a file or a setting is ever turned into a command name. */
const ALLOWED = new Set(["blame", "why", "log", "status", "pending", "record", "note", "undo", "show", "companion"]);

export type Failure = "missing" | "old" | "not-a-project" | "failed" | "timeout" | "not-allowed";

export type Result = { ok: true; json: unknown } | { ok: false; reason: Failure; message: string };

export interface RunOptions {
  /** The `tracewise` program: a path, or just `tracewise` for the one on PATH. */
  program: string;
  /** The folder the file is in: Tracewise finds the project from there, so a subfolder workspace works. */
  cwd: string;
  timeoutMs: number;
}

function fail(reason: Failure, message: string): Result {
  return { ok: false, reason, message };
}

function exec(program: string, args: string[], cwd: string | undefined, timeoutMs: number): Promise<Result & { code?: number }> {
  return new Promise((resolve) => {
    execFile(
      program,
      args,
      { cwd, timeout: timeoutMs, maxBuffer: 64 * 1024 * 1024, env: { ...process.env, NO_COLOR: "1" }, windowsHide: true },
      (error, stdout, stderr) => {
        if (error) {
          const e = error as Error & { killed?: boolean; code?: number | string };
          if (e.code === "ENOENT") return resolve(fail("missing", `${program} was not found`));
          if (e.killed) return resolve(fail("timeout", `${args[0]} took longer than ${timeoutMs} ms`));
          if (e.code === 2) return resolve({ ...fail("not-a-project", stderr.trim() || "not a Tracewise project"), code: 2 });
          // a git repository where `tracewise init` was never run: for the extension, the same as no project
          if (/no Tracewise history yet/.test(stderr)) return resolve({ ...fail("not-a-project", stderr.trim()), code: 1 });
          return resolve({ ...fail("failed", stderr.trim() || e.message), code: typeof e.code === "number" ? e.code : undefined });
        }
        try {
          resolve({ ok: true, json: JSON.parse(stdout) });
        } catch {
          resolve(fail("failed", `${args[0]} did not answer with JSON`));
        }
      },
    );
  });
}

/** Runs `tracewise -C <cwd> <args…> --json`. */
export function run(args: string[], opts: RunOptions): Promise<Result> {
  if (!args.length || !ALLOWED.has(args[0])) return Promise.resolve(fail("not-allowed", `the extension does not run \`${args[0]}\``));
  // a folder that is gone would otherwise look like a missing program (both are ENOENT)
  if (!existsSync(opts.cwd)) return Promise.resolve(fail("failed", `${opts.cwd} does not exist`));
  return exec(opts.program, ["-C", opts.cwd, ...args, "--json"], opts.cwd, opts.timeoutMs);
}

export interface Companion {
  api: number;
  features: string[];
}

const companions = new Map<string, Promise<Companion | Failure>>();

/**
 * What this `tracewise` can do (`companion --json`), asked once per program per session while it works. A failure is not
 * remembered: installing or updating the program takes effect without reloading the window.
 */
export function companionOf(program: string, timeoutMs = 3000): Promise<Companion | Failure> {
  let known = companions.get(program);
  if (!known) {
    known = exec(program, ["companion", "--json"], undefined, timeoutMs).then((r): Companion | Failure => {
      if (!r.ok) return r.reason === "missing" ? "missing" : "old";
      const info = r.json as { api?: unknown; features?: unknown };
      if (!Array.isArray(info.features)) return "old";
      return { api: typeof info.api === "number" ? info.api : 0, features: info.features.filter((f): f is string => typeof f === "string") };
    });
    companions.set(program, known);
    void known.then((result) => {
      if (typeof result === "string") companions.delete(program);
    });
  }
  return known;
}

/** `ok`, or why the extension cannot use this program: not found, too old for what it needs, or newer than the extension knows. */
export async function check(program: string, needs: string[]): Promise<"ok" | "missing" | "old" | "newer"> {
  const have = await companionOf(program);
  if (have === "missing") return "missing";
  if (typeof have === "string") return "old";
  if (have.api > KNOWN_API) return "newer";
  return needs.every((n) => have.features.includes(n)) ? "ok" : "old";
}

/** For tests: forget what was learned about programs. */
export function forgetPrograms(): void {
  companions.clear();
}
