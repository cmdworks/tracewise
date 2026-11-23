<script lang="ts">
  import Icon from "$lib/Icon.svelte";
  import { app } from "$lib/app/state.svelte";
  import * as fx from "$lib/app/fx";
  import type { ChangeItem } from "$lib/types";

  let { c, isBaseline }: { c: ChangeItem; isBaseline: boolean } = $props();

  let noteInput = $state("");
  let editing = $state(false);
  let editorEl = $state<HTMLTextAreaElement>();

  // another change picked: leave edit mode and forget the draft
  $effect(() => {
    void app.selectedChange?.id;
    editing = false;
  });

  function startEdit() {
    noteInput = app.selectedChange?.why_auto ? "" : app.selectedChange?.why || "";
    editing = true;
    queueMicrotask(() => editorEl?.focus());
  }

  async function handleSaveReason() {
    if (!app.selectedChange) return;
    app.inlineNoteInput = noteInput;
    // a refused reason keeps the editor open with the text, so it can be fixed
    if (await fx.saveInlineNote()) editing = false;
  }
</script>

<!-- The reason: read it, then Add or Edit; Save and Cancel only while editing -->
<div class="why-section">
  <div class="why-head">
    <span class="meta-label text-12 font-semibold">WHY</span>
    {#if !editing && !isBaseline}
      <button type="button" class="why-action text-12" class:add={!c.why || c.why_auto} onclick={startEdit}>
        <Icon name="pencil" size={12} />
        <span>{c.why && !c.why_auto ? "Edit" : "Add reason"}</span>
      </button>
    {/if}
  </div>

  {#if editing}
    <div class="why-editor">
      <textarea
        bind:this={editorEl}
        bind:value={noteInput}
        class="why-input text-13"
        rows="3"
        placeholder="Why was this changed? For example: Fix the off-by-one in page counts"
        aria-label="Reason for this change"
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            handleSaveReason();
          } else if (e.key === "Escape") {
            e.stopPropagation();
            editing = false;
          }
        }}
      ></textarea>
      <div class="why-editor-foot">
        <span class="text-12 text-muted">
          {c.why && !c.why_auto ? "Replaces the reason for this change; the old one stays in the history." : "At least 12 characters; say why, not what."}
        </span>
        <div class="why-buttons">
          <button type="button" class="why-cancel text-12" onclick={() => (editing = false)}>Cancel</button>
          <button
            type="button"
            class="why-save text-12"
            disabled={app.inlineNoteSaving || !noteInput.trim() || noteInput.trim() === (c.why || "")}
            onclick={handleSaveReason}
          >
            {app.inlineNoteSaving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  {:else if c.why}
    <p class="why-text text-13 select-text" class:auto-reason={c.why_auto}>{c.why}{#if c.why_auto} <span class="text-muted">(automatic: nobody was asked)</span>{/if}</p>
  {:else if isBaseline}
    <p class="why-text text-13 text-muted">Initial project baseline</p>
  {:else}
    <p class="why-missing text-12"><span class="amber-dot"></span>No reason yet</p>
  {/if}
  {#if c.command}
    <div class="command-banner font-mono text-12 select-text">
      <span class="text-muted">$</span> {c.command}
    </div>
  {/if}
</div>

<style>
  .why-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 24px;
  }
  .why-action {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: none;
    color: var(--muted-foreground);
    cursor: pointer;
  }
  .why-action:hover {
    border-color: var(--border);
    color: var(--foreground);
  }
  .why-action.add {
    border-color: var(--warning-border);
    color: var(--warning);
  }
  .auto-reason {
    font-style: italic;
    opacity: 0.8;
  }
  .why-text {
    margin: 6px 0 0;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .why-missing {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 6px 0 0;
    color: var(--warning);
  }
  .why-missing .amber-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warning);
  }
  .why-editor {
    margin-top: 6px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .why-input {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    padding: 8px 10px;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: var(--bg);
    color: var(--foreground);
    font-family: var(--font-ui);
    line-height: 1.5;
    outline: none;
  }
  .why-editor-foot {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }
  .why-buttons {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .why-save {
    height: 28px;
    padding: 0 12px;
    border-radius: 4px;
    cursor: pointer;
  }
  .why-cancel {
    border: 1px solid var(--border);
    background: none;
    color: var(--foreground);
  }
  .why-save {
    border: 1px solid var(--accent);
    background: var(--accent);
    color: #fff;
  }
  .why-save:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .why-section {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .command-banner {
    padding: 4px 8px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--foreground);
  }
  .meta-label {
    letter-spacing: 0.05em;
    color: var(--muted-foreground);
    text-transform: uppercase;
  }
</style>
