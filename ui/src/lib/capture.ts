// What `tracewise capture --json` says, in the words the Health tab shows. The app only reads it: changing the preset is `tracewise capture use`.
export interface CaptureStatus {
  /** The preset the installed layers match, or null for a setup that matches none. */
  preset: string | null;
  layers: { terminal: boolean; daemon: boolean; watcher_running: boolean; hooks: string[] };
  /** `full` or `agents`. */
  mode: string;
  /** Things that disagree with each other, in words. */
  problems: string[];
  warnings: string[];
}

export interface CaptureLine {
  name: string;
  on: boolean;
  text: string;
}

/** The layers one by one, each with whether it is on and what that means. */
export function captureLines(c: CaptureStatus): CaptureLine[] {
  const { terminal, daemon, watcher_running, hooks } = c.layers;
  return [
    { name: "Agent hooks", on: hooks.length > 0, text: hooks.length ? `installed for ${hooks.join(", ")}` : "none installed: agents' edits cannot be credited exactly" },
    { name: "Terminal", on: terminal, text: terminal ? "commands you type and git are recorded as they happen" : "Tracewise's lines are not in your shell startup files" },
    { name: "Watcher", on: daemon, text: !daemon ? "no background process" : watcher_running ? "running: changes by any program are recorded" : "installed but not running" }
  ];
}

/** The headline: the preset's name, or why there is none. */
export function captureHeadline(c: CaptureStatus | null): string {
  if (!c) return "Capture status is not loaded yet.";
  return c.preset ? `Capture: ${c.preset}` : "Capture: custom (no preset matches what is installed)";
}

/** Reads the answer of `tracewise capture --json`; anything else is null. */
export function parseCapture(raw: unknown): CaptureStatus | null {
  const r = raw as Partial<CaptureStatus> | null;
  if (!r || typeof r !== "object" || !r.layers || typeof r.mode !== "string") return null;
  const layers = r.layers as Partial<CaptureStatus["layers"]>;
  return {
    preset: typeof r.preset === "string" ? r.preset : null,
    layers: {
      terminal: layers.terminal === true,
      daemon: layers.daemon === true,
      watcher_running: layers.watcher_running === true,
      hooks: Array.isArray(layers.hooks) ? layers.hooks.filter((h): h is string => typeof h === "string") : []
    },
    mode: r.mode,
    problems: Array.isArray(r.problems) ? r.problems.filter((p): p is string => typeof p === "string") : [],
    warnings: Array.isArray(r.warnings) ? r.warnings.filter((p): p is string => typeof p === "string") : []
  };
}
