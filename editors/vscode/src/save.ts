import * as path from "node:path";

/**
 * The path to hand to `tracewise record`: relative to the workspace folder that holds the file, or nothing when the file is in none
 * of them (or the setting is off). Tracewise decides what is ignored; this only keeps unrelated files out.
 */
export function pathToRecord(file: string, folders: string[], enabled: boolean): string | undefined {
  if (!enabled) return undefined;
  for (const folder of folders) {
    const rel = path.relative(folder, file);
    if (rel && !rel.startsWith("..") && !path.isAbsolute(rel)) return file;
  }
  return undefined;
}

export interface Timers {
  set(fn: () => void, ms: number): unknown;
  clear(handle: unknown): void;
}

export const realTimers: Timers = {
  set: (fn, ms) => setTimeout(fn, ms),
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

/** Collapses a burst of saves of one file into one call, `ms` after the last of them. */
export function debouncedPerKey(ms: number, fn: (key: string) => void, timers: Timers = realTimers): (key: string) => void {
  const waiting = new Map<string, unknown>();
  return (key) => {
    const previous = waiting.get(key);
    if (previous !== undefined) timers.clear(previous);
    waiting.set(
      key,
      timers.set(() => {
        waiting.delete(key);
        fn(key);
      }, ms),
    );
  };
}
