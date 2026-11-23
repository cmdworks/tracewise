import { app } from "./state.svelte";
import { parseSession, SESSION_KEY } from "$lib/session";
import { stepSelection, toggleSidebar } from "./fx-core";
import { fetchAgents, fetchBackground, fetchCapture, fetchDaemonStatus, fetchDoctor, fetchLocks, fetchPending, fetchStatus, fetchTimeline, openFileBlame, refreshAll, pollStatus, switchRepo } from "./fx-load";
import { formatRelativeTime, isStatusStale } from "$lib/live";
import { openUndoModal } from "./fx-act";
import { checkCli } from "./fx-cli";

/** Loads everything the panels show; runs once the command line is there. */
export function loadAll(): void {
    fetchDaemonStatus();
    fetchCapture();
    fetchStatus();
    fetchTimeline();
    fetchAgents();
    // the first change is selected on load; in a narrow window its inspector would cover the list, so it waits for a click
    app.inspectorVisible = window.innerWidth >= 1100;
    fetchLocks(); // the Timeline marks locked files, so it needs them before Now is opened
    fetchPending();
    fetchDoctor();
    if (app.currentRepo && app.activeTab === "files" && app.selectedFilePath) openFileBlame(app.selectedFilePath);
}

/** Starts the page: reads the saved layout, loads everything, listens for keys. Returns what stops it. */
export function start(): () => void {
    if (typeof localStorage !== "undefined") {
      const savedCollapsed = localStorage.getItem("tracewise-sidebar-collapsed");
      if (savedCollapsed === "1") app.sidebarCollapsed = true;
      app.blameWrap = localStorage.getItem("tracewise-blame-wrap") !== "0";
    }
    // come back to where the last session (or the last hot reload) left off: the same project and tab, and the open file
    const saved = parseSession(typeof localStorage === "undefined" ? null : localStorage.getItem(SESSION_KEY));
    if (saved.repo) {
      app.currentRepo = saved.repo;
      app.repoRestored = true;
      app.activeTab = saved.tab;
      if (saved.file) app.selectedFilePath = saved.file;
    }
    app.sessionReady = true;

    // nothing is loaded before the command line is found; when it is missing the page offers to install it
    checkCli().then((found) => {
      if (found) loadAll();
    });

    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        app.showUndoModal = false;
        app.showShortcutsModal = false;
        app.showJsonModal = false;
      }
      // while a dialog is open the keys belong to the dialog, not to the page behind it
      if (app.showUndoModal || app.showShortcutsModal || app.showJsonModal) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "b") {
        e.preventDefault();
        toggleSidebar();
        return;
      }

      const active = document.activeElement as HTMLElement | null;
      const tag = active?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || active?.isContentEditable) return;

      if (e.key === "j" || (e.key === "ArrowDown" && app.activeTab === "timeline")) {
        e.preventDefault();
        stepSelection(1);
      } else if (e.key === "k" || (e.key === "ArrowUp" && app.activeTab === "timeline")) {
        e.preventDefault();
        stepSelection(-1);
      } else if (e.key === "r") {
        refreshAll();
      } else if (e.key === "u" && app.selectedChange) {
        openUndoModal(app.selectedChange);
      } else if (e.key === "b" && app.selectedChange) {
        openFileBlame(app.selectedChange.path);
      } else if (e.key === "i") {
        app.inspectorVisible = !app.inspectorVisible;
      } else if (e.key === "?") {
        app.showShortcutsModal = !app.showShortcutsModal;
      } else if (e.key === "1") {
        app.activeTab = "timeline";
      } else if (e.key === "2") {
        app.activeTab = "now";
        fetchAgents();
        fetchPending();
        fetchLocks();
        fetchBackground();
      } else if (e.key === "3") {
        if (app.selectedFilePath) openFileBlame(app.selectedFilePath);
        else app.activeTab = "files";
      } else if (e.key === "4") {
        app.activeTab = "undo";
      } else if (e.key === "5") {
        app.activeTab = "projects";
        fetchDaemonStatus();
      } else if (e.key === "6") {
        app.activeTab = "health";
        fetchDoctor();
        fetchCapture();
      } else if (e.key === "/") {
        e.preventDefault();
        const el = document.getElementById("search-input");
        el?.focus();
      }
    };
    window.addEventListener("keydown", handleKey);

    // The sidebar follows the window while nobody has chosen: a narrow window gives the room to the content, a wide one gets it back.
    // Collapsing or expanding it by hand saves a choice, and from then on the width no longer decides.
    const narrow = window.matchMedia("(max-width: 1099px)");
    const followWidth = () => {
      if (localStorage.getItem("tracewise-sidebar-collapsed") === null) app.sidebarCollapsed = narrow.matches;
    };
    followWidth();
    narrow.addEventListener("change", followWidth);
    window.addEventListener("resize", followWidth); // some webviews do not fire the media query's change

    // The desktop app is told when a history changes ("history-changed", sent by the Rust side when the system reports a write), so it
    // reads again then and not on a timer. A slow check remains in case a report is missed; a browser during development has no such
    // event, so there it asks every few seconds.
    const pollTimer = setInterval(
      () => {
        if (typeof document !== "undefined" && document.visibilityState === "visible") pollStatus();
      },
      import.meta.env.DEV ? 3000 : 30000
    );
    // the "x seconds ago" text moves on its own; this asks nothing
    const clockTimer = setInterval(() => {
      if (app.lastStatusTime) {
        const now = new Date();
        app.relativeStatusTime = formatRelativeTime(app.lastStatusTime, now);
        app.statusStale = isStatusStale(app.lastStatusTime, now);
      }
    }, 5000);

    const handleFocus = () => {
      pollStatus();
    };
    const handleVisibility = () => {
      if (typeof document !== "undefined" && document.visibilityState === "visible") {
        pollStatus();
      }
    };
    window.addEventListener("focus", handleFocus);
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", handleVisibility);
    }

    // choosing a project in the menu bar switches the window to it too
    let stopListening: (() => void) | undefined;
    let stopHistory: (() => void) | undefined;
    // a window that is not shown reads again when it comes back (the focus and visibility handlers below), not for every change
    const readAgain = () => {
      if (typeof document === "undefined" || document.visibilityState === "visible") pollStatus();
    };
    import("@tauri-apps/api/event")
      .then(async ({ listen }) => {
        stopListening = await listen<string>("switch-repo", (e) => switchRepo(e.payload));
        stopHistory = await listen("history-changed", readAgain);
      })
      .catch(() => {}); // not in the desktop app (a browser during development): there is no menu bar and no event

    return () => {
      stopListening?.();
      stopHistory?.();
      clearInterval(pollTimer);
      clearInterval(clockTimer);
      window.removeEventListener("focus", handleFocus);
      if (typeof document !== "undefined") {
        document.removeEventListener("visibilitychange", handleVisibility);
      }
      window.removeEventListener("keydown", handleKey);
      narrow.removeEventListener("change", followWidth);
      window.removeEventListener("resize", followWidth);
    };
  }
