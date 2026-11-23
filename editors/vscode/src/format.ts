// What the extension shows, as plain functions: no VS Code API here, so they can be tested without an editor.

export interface Actor {
  kind: string;
  name?: string | null;
  session?: string | null;
}

/** One line of `tracewise blame --json`. */
export interface BlameRow {
  line: number;
  text: string;
  /** A change id, or `baseline` for a line from before Tracewise started. */
  change: string;
  actor: Actor;
  time: string;
  why: string | null;
  git_author?: string | null;
}

/** Consecutive lines that one change wrote; `start` and `end` are 1-based and inclusive. */
export interface Run {
  start: number;
  end: number;
  row: BlameRow;
}

export function actorLabel(actor: Actor): string {
  switch (actor.kind) {
    case "human":
      return actor.name ?? "someone";
    case "agent":
      return actor.name ?? "an agent";
    case "baseline":
      return "before Tracewise";
    case "ambiguous":
      return "unclear who";
    default:
      return "unknown";
  }
}

export function relativeTime(iso: string, now: Date): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const seconds = Math.max(0, Math.round((now.getTime() - then) / 1000));
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  if (days < 14) return `${days}d ago`;
  const weeks = Math.round(days / 7);
  if (weeks < 9) return `${weeks}w ago`;
  const months = Math.round(days / 30);
  if (months < 18) return `${months}mo ago`;
  return `${Math.round(days / 365)}y ago`;
}

/** `claude · 2h ago · handle empty input`: the end-of-line label for a run. */
export function blameLabel(row: BlameRow, now: Date): string {
  const parts = [actorLabel(row.actor)];
  const when = relativeTime(row.time, now);
  if (when) parts.push(when);
  if (row.why) parts.push(row.why);
  else if (row.actor.kind === "baseline" && row.git_author) parts.push(row.git_author);
  return parts.join(" · ");
}

/** Lines written by the same change, in order. A change that wrote two separate stretches is two runs. */
export function groupRuns(rows: BlameRow[]): Run[] {
  const runs: Run[] = [];
  for (const row of rows) {
    const last = runs[runs.length - 1];
    if (last && last.row.change === row.change && last.end + 1 === row.line) last.end = row.line;
    else runs.push({ start: row.line, end: row.line, row });
  }
  return runs;
}

/** True for a string that is a change id: what the extension is willing to pass to `tracewise undo`. */
export function isChangeId(text: string): boolean {
  return /^[0-9A-Z]{26}$/.test(text);
}
