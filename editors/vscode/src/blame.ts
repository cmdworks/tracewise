import { blameLabel, actorLabel, groupRuns, relativeTime, type BlameRow } from "./format.ts";

/** Above this many lines a file is not annotated: the answer would be slow and the margin unreadable. */
export const MAX_BLAME_LINES = 50_000;

export interface Label {
  /** 0-based line the label sits at the end of (the first line of its run). */
  line: number;
  text: string;
  hover: string;
  change: string;
}

/**
 * One label per run of lines, at the run's first line. Lines from before Tracewise started carry no label: they are most of
 * a file and would drown out the lines that matter.
 */
export function labelsFor(rows: BlameRow[], now: Date): Label[] {
  return groupRuns(rows)
    .filter((run) => run.row.change !== "baseline")
    .map((run) => ({
      line: run.start - 1,
      text: blameLabel(run.row, now),
      hover: hoverText(run.row, run.end - run.start + 1, now),
      change: run.row.change,
    }));
}

function hoverText(row: BlameRow, lines: number, now: Date): string {
  const when = relativeTime(row.time, now);
  const head = `**${actorLabel(row.actor)}**${when ? ` · ${when}` : ""} · ${lines} line${lines === 1 ? "" : "s"}`;
  const why = row.why ? `\n\n${row.why}` : "\n\n_no reason given_";
  return `${head}${why}\n\nchange \`${row.change}\``;
}

/** The change behind a 0-based line of the file, if there is one worth acting on. */
export function changeAtLine(rows: BlameRow[], line0: number): BlameRow | undefined {
  const row = rows.find((r) => r.line === line0 + 1);
  return row && row.change !== "baseline" ? row : undefined;
}

/** Blame is skipped for a file with more lines than this. */
export function tooBig(lineCount: number): boolean {
  return lineCount > MAX_BLAME_LINES;
}
