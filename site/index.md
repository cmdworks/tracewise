---
layout: home
title: Tracewise
titleTemplate: A flight recorder for your code

hero:
  name: Tracewise
  text: A flight recorder for your code
  tagline: Who changed every line, when, and why — you, a coding agent, a script or git. Recorded. Explained. Reversible.
  actions:
    - theme: brand
      text: Get started
      link: /docs/getting-started
    - theme: alt
      text: Download
      link: /download
    - theme: alt
      text: Use it
      link: "#showcase"
    - theme: alt
      text: View on GitHub
      link: https://github.com/cmdworks/tracewise

features:
  - icon:
      src: /icons/search.svg
      width: 28
      height: 28
    title: Every change, attributed
    details: Each file change is recorded with its author (you, Claude Code, Codex, Cursor, Gemini, Antigravity, a script or git), how sure Tracewise is, and a reason.
  - icon:
      src: /icons/undo.svg
      width: 28
      height: 28
    title: Undo by line, not by file
    details: Take back one change, or everything an agent did in the last hour. Only the lines it wrote go; your later edits stay.
  - icon:
      src: /icons/lock.svg
      width: 28
      height: 28
    title: Locks between agents
    details: Optional locks so two agents never edit the same file at once. Refused edits are recorded too.
  - icon:
      src: /icons/verified.svg
      width: 28
      height: 28
    title: Tamper-evident history
    details: A hash-chained record kept next to your project, not in your commits, and `doctor --verify` proves nothing was altered. It works in folders that are not git repositories too.
  - icon:
      src: /icons/share.svg
      width: 28
      height: 28
    title: Share between machines
    details: Opt-in sharing of history with signed, optionally encrypted batches.
  - icon:
      src: /icons/desktop.svg
      width: 28
      height: 28
    title: CLI, desktop app and editors
    details: Everything from the terminal, a live timeline and a menu-bar icon in the desktop app, and blame on every line in VS Code and Zed (`you via Zed`).
---
