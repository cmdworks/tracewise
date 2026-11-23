import * as path from "node:path";
import * as vscode from "vscode";
import { changeAtLine, labelsFor, tooBig } from "./blame.ts";
import { check, run, type Result } from "./cli.ts";
import { actorLabel, isChangeId, type BlameRow } from "./format.ts";
import { debouncedPerKey, pathToRecord } from "./save.ts";
import { viewOf, type State } from "./status.ts";

const READ_TIMEOUT_MS = 3000;
const RECORD_TIMEOUT_MS = 10_000;

/** What blame said about each open, saved file: so a command can find the change under the cursor without asking again. */
const blameOf = new Map<string, BlameRow[]>();
/** Bumped for every blame of a file: an answer that arrives after a newer one started is dropped. */
const generation = new Map<string, number>();

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel("Tracewise");
  const bar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
  const decoration = vscode.window.createTextEditorDecorationType({
    after: { margin: "0 0 0 2.5em", color: new vscode.ThemeColor("editorCodeLens.foreground"), fontStyle: "italic" },
  });
  context.subscriptions.push(output, bar, decoration);

  const settings = () => vscode.workspace.getConfiguration("tracewise");
  const program = () => settings().get<string>("path")?.trim() || "tracewise";
  const folderOf = (file: string) => path.dirname(file);

  /** Whether the extension may run Tracewise now: not in a folder the person has not trusted. */
  async function use(needs: string[]): Promise<"ok" | "missing" | "old" | "newer" | "untrusted"> {
    if (!vscode.workspace.isTrusted) return "untrusted";
    return check(program(), needs);
  }

  function show(state: State): void {
    const view = viewOf(state);
    bar.text = view.text;
    bar.tooltip = view.tooltip;
    bar.command = view.command;
    bar.show();
  }

  function stateOfFailure(r: Extract<Result, { ok: false }>): State {
    if (r.reason === "not-a-project") return { kind: "not-tracked" };
    if (r.reason === "missing") return { kind: "missing" };
    return { kind: "error", message: r.message };
  }

  async function refreshStatus(editor: vscode.TextEditor | undefined): Promise<void> {
    const file = editor?.document.uri.scheme === "file" ? editor.document.uri.fsPath : undefined;
    if (!file) return;
    const usable = await use(["json-contract"]);
    if (usable !== "ok") return show({ kind: usable });
    const opts = { program: program(), cwd: folderOf(file), timeoutMs: READ_TIMEOUT_MS };
    const status = await run(["status"], opts);
    if (!status.ok) return show(stateOfFailure(status));
    const pending = await run(["pending"], opts);
    const mode = (status.json as { capture?: { mode?: string } }).capture?.mode ?? "full";
    show({ kind: "tracked", mode, pending: pending.ok && Array.isArray(pending.json) ? pending.json.length : 0 });
  }

  function clearLabels(file: string): void {
    blameOf.delete(file);
    generation.set(file, (generation.get(file) ?? 0) + 1);
    for (const editor of vscode.window.visibleTextEditors) {
      if (editor.document.uri.fsPath === file) editor.setDecorations(decoration, []);
    }
  }

  async function refreshBlame(editor: vscode.TextEditor | undefined): Promise<void> {
    if (!editor || editor.document.uri.scheme !== "file") return;
    const document = editor.document;
    const file = document.uri.fsPath;
    clearLabels(file);
    // the labels describe the file on disk: with unsaved edits the lines would be off, so there are none until it is saved
    if (!settings().get<boolean>("blame.enabled", true) || document.isDirty || tooBig(document.lineCount)) return;
    if ((await use(["json-contract"])) !== "ok") return;
    const mine = generation.get(file) ?? 0;
    const version = document.version;
    const result = await run(["blame", file], { program: program(), cwd: folderOf(file), timeoutMs: READ_TIMEOUT_MS });
    // typing, or a newer refresh, started while the answer was on its way: it describes a file that is not on screen any more
    if (!result.ok || !Array.isArray(result.json) || generation.get(file) !== mine || document.version !== version || document.isDirty) return;
    const rows = result.json as BlameRow[];
    blameOf.set(file, rows);
    const now = new Date();
    const options: vscode.DecorationOptions[] = labelsFor(rows, now)
      .filter((label) => label.line < document.lineCount)
      .map((label) => {
        const range = document.lineAt(label.line).range;
        return {
          range: new vscode.Range(range.end, range.end),
          hoverMessage: new vscode.MarkdownString(label.hover),
          renderOptions: { after: { contentText: label.text } },
        };
      });
    editor.setDecorations(decoration, options);
  }

  async function refresh(editor = vscode.window.activeTextEditor): Promise<void> {
    await Promise.all([refreshBlame(editor), refreshStatus(editor)]);
  }

  const record = debouncedPerKey(300, (file) => {
    void (async () => {
      if ((await use(["record"])) !== "ok") return;
      const r = await run(["record", "--source", "vscode", file], { program: program(), cwd: folderOf(file), timeoutMs: RECORD_TIMEOUT_MS });
      if (!r.ok && r.reason !== "not-a-project") output.appendLine(`record ${file}: ${r.message}`);
      await refresh();
      if (r.ok && settings().get<boolean>("askWhySave", false)) {
        const ids = ((r.json as { changes?: unknown })?.changes ?? []) as unknown[];
        const changes = ids.filter((i): i is string => typeof i === "string");
        if (!changes.length) return;
        const why = await vscode.window.showInputBox({ prompt: `Tracewise: why did you change ${path.basename(file)}?`, placeHolder: "a short reason (Esc to skip)" });
        if (!why?.trim()) return;
        for (const id of changes) {
          const n = await run(["note", id, why.trim()], { program: program(), cwd: folderOf(file), timeoutMs: RECORD_TIMEOUT_MS });
          if (!n.ok) output.appendLine(`note ${id}: ${n.message}`);
        }
        await refresh();
      }
    })();
  });

  /** A file reloaded from disk (an agent or git changed it): the labels are stale, refresh once the changes stop. */
  const reloaded = debouncedPerKey(300, (file) => {
    const editor = vscode.window.visibleTextEditors.find((e) => e.document.uri.fsPath === file);
    if (editor && !editor.document.isDirty) void refresh(editor);
  });

  const lineChange = (): { file: string; line: number; row: BlameRow } | undefined => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.uri.scheme !== "file" || editor.document.isDirty) return undefined;
    const file = editor.document.uri.fsPath;
    const row = changeAtLine(blameOf.get(file) ?? [], editor.selection.active.line);
    return row ? { file, line: editor.selection.active.line + 1, row } : undefined;
  };

  context.subscriptions.push(
    vscode.commands.registerCommand("tracewise.refresh", () => refresh()),
    vscode.commands.registerCommand("tracewise.showChange", async () => {
      const at = lineChange();
      if (!at) return void vscode.window.showInformationMessage("Tracewise: no recorded change for this line (save the file first if you have edited it).");
      const r = await run(["why", `${at.file}:${at.line}`], { program: program(), cwd: folderOf(at.file), timeoutMs: READ_TIMEOUT_MS });
      if (!r.ok) return void vscode.window.showWarningMessage(`Tracewise: ${r.message}`);
      const doc = await vscode.workspace.openTextDocument({ language: "json", content: JSON.stringify(r.json, null, 2) });
      await vscode.window.showTextDocument(doc, { preview: true, viewColumn: vscode.ViewColumn.Beside });
    }),
    vscode.commands.registerCommand("tracewise.undoChange", async () => {
      const at = lineChange();
      if (!at || !isChangeId(at.row.change)) {
        return void vscode.window.showInformationMessage("Tracewise: no recorded change for this line (save the file first if you have edited it).");
      }
      const opts = { program: program(), cwd: folderOf(at.file), timeoutMs: RECORD_TIMEOUT_MS };
      // ask first whether it can be undone at all: a change that was hash-only or conflicts is refused here, with the reason
      const preview = await run(["undo", at.row.change, "--dry-run"], opts);
      if (!preview.ok) return void vscode.window.showWarningMessage(`Tracewise cannot undo this change: ${preview.message}`);
      const who = actorLabel(at.row.actor);
      const detail = at.row.why ? `${who}: ${at.row.why}` : who;
      const answer = await vscode.window.showWarningMessage(
        `Undo change ${at.row.change}?`,
        { modal: true, detail: `${detail}\n\nOnly the lines this change wrote are taken back; your later edits to other lines stay.` },
        "Undo",
      );
      if (answer !== "Undo") return;
      const r = await run(["undo", at.row.change], opts);
      if (!r.ok) vscode.window.showWarningMessage(`Tracewise: ${r.message}`);
      await refresh();
    }),
    vscode.commands.registerCommand("tracewise.showPending", async () => {
      const file = vscode.window.activeTextEditor?.document.uri.fsPath;
      if (!file) return;
      const opts = { program: program(), cwd: folderOf(file), timeoutMs: READ_TIMEOUT_MS };
      const [pending, status] = await Promise.all([run(["pending"], opts), run(["status"], opts)]);
      if (!pending.ok || !Array.isArray(pending.json)) return;
      // the paths are relative to the project, which is not always the folder VS Code opened
      const root = status.ok ? (status.json as { repo?: string }).repo : undefined;
      if (!root) return;
      const rows = pending.json as { id: string; path: string; actor: { kind: string; name?: string } }[];
      const picked = await vscode.window.showQuickPick(
        rows.map((p) => ({ label: p.path, description: actorLabel(p.actor), id: p.id })),
        { placeHolder: 'Changes without a reason (tracewise note <id> "why" adds one)' },
      );
      if (picked) await vscode.window.showTextDocument(vscode.Uri.file(path.join(root, picked.label)));
    }),
    vscode.window.onDidChangeActiveTextEditor((editor) => void refresh(editor)),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (e.document.uri.scheme !== "file" || e.contentChanges.length === 0) return;
      const file = e.document.uri.fsPath;
      // typing: the labels no longer match what is on screen. Not dirty after a change: the file was reloaded from disk.
      if (e.document.isDirty) clearLabels(file);
      else reloaded(file);
    }),
    vscode.workspace.onDidCloseTextDocument((doc) => {
      blameOf.delete(doc.uri.fsPath);
      generation.delete(doc.uri.fsPath);
    }),
    vscode.workspace.onDidSaveTextDocument((doc) => {
      if (doc.uri.scheme !== "file") return;
      const folders = (vscode.workspace.workspaceFolders ?? []).map((f) => f.uri.fsPath);
      const file = pathToRecord(doc.uri.fsPath, folders, settings().get<boolean>("recordSaves", true));
      if (file) record(file);
      else void refresh();
    }),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("tracewise")) void refresh();
    }),
    vscode.workspace.onDidGrantWorkspaceTrust(() => void refresh()),
  );

  void refresh();
}

export function deactivate(): void {}
