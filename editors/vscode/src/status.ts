export type State =
  | { kind: "tracked"; mode: string; pending: number }
  | { kind: "not-tracked" }
  | { kind: "missing" }
  | { kind: "old" }
  | { kind: "newer" }
  | { kind: "untrusted" }
  | { kind: "error"; message: string };

export interface View {
  text: string;
  tooltip: string;
  /** The command a click runs; none for states with nothing to open. */
  command?: string;
}

/** The status bar item for each state. Outside a project it is quiet: no warning colour, no popup. */
export function viewOf(state: State): View {
  switch (state.kind) {
    case "tracked":
      return {
        text: state.pending > 0 ? `$(history) Tracewise · ${state.pending} without a reason` : "$(history) Tracewise",
        tooltip: `Capture mode: ${state.mode}. ${state.pending} change${state.pending === 1 ? "" : "s"} without a reason.`,
        command: state.pending > 0 ? "tracewise.showPending" : "tracewise.refresh",
      };
    case "not-tracked":
      return { text: "$(history) Tracewise: not tracked", tooltip: "This folder is not a Tracewise project. Run `tracewise init` in it." };
    case "missing":
      return { text: "$(warning) Tracewise: not installed", tooltip: "The tracewise program was not found. Install it, or set tracewise.path." };
    case "old":
      return { text: "$(warning) Tracewise: update", tooltip: "This tracewise is too old for the extension. Update it." };
    case "newer":
      return { text: "$(warning) Tracewise: update the extension", tooltip: "This tracewise is newer than the extension understands. Update the extension." };
    case "untrusted":
      return {
        text: "$(shield) Tracewise: trust this folder",
        tooltip: "Tracewise runs a program on your machine, and it reads this folder's git settings. It is off until you trust the folder.",
        command: "workbench.trust.manage",
      };
    case "error":
      return { text: "$(warning) Tracewise", tooltip: state.message };
  }
}
