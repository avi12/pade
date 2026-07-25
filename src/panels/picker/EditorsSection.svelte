<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { ideBrand, ideIcon } from "@/lib/ide-icon";
  import { languageIcon } from "@/lib/language-icon";
  import { showToast } from "@/lib/stores/toast.svelte";
  import type { EditorKind, Ide, Prefs } from "@/lib/types";
  import { FolderPath, parseInput } from "@/lib/validate";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  // Editor-rules engine — the project kinds come from the backend registry
  // (`ide_kinds`, priority order), one row per kind. A rule maps a kind to an
  // editor id; unmatched folders use the fallback. Each kind carries the
  // manifest files PADE looks for to classify a folder.
  // Rule/fallback persistence goes through parent callbacks into shared settings;
  // this section renders the rows and reports picks.
  const {
    ides,
    kinds,
    kindOptions,
    prefs,
    onrule,
    onfallback,
    onaddeditor,
    onrescan,
    onremoveeditor
  }: {
    ides: Ide[];
    /** The project kinds to render rows for (label + manifest signals), straight
        from the backend registry in its render/priority order — the single home
        of the kind list, so a new kind needs no frontend change. */
    kinds: EditorKind[];
    /** Editor ids that suit each project kind (kind → ordered, installed-only), so
        a kind's menu offers only fitting editors — no WebStorm on an Android row. */
    kindOptions: Record<string, string[]>;
    prefs: Prefs;
    onrule: (rule: {
      kind: string;
      editorId: string;
    }) => void;
    onfallback: (editorId: string) => void;
    /** Add an editor by executable path — resolves to its label or a rejection. */
    onaddeditor: (path: string) => Promise<{
      label: string;
    } | {
      error: string;
    }>;
    /** Re-probe the machine for installed editors — resolves to how many were
        found, so the button can report the count. */
    onrescan: () => Promise<number>;
    /** Drop a user-added editor by id (removes it from every menu). */
    onremoveeditor: (id: string) => Promise<void>;
  } = $props();

  // "Add editor…" flow — reveal an inline path field, validate & persist the
  // executable through the backend, and surface an ok/error status line.
  const StatusKind = {
    Ok: "ok",
    Error: "err"
  } as const;
  type StatusKind = (typeof StatusKind)[keyof typeof StatusKind];

  let adding = $state(false);
  let draft = $state("");
  let status = $state<{
    kind: StatusKind;
    text: string;
  } | null>(null);
  // Re-detect in flight — flips the header's Reload button to "Detecting…".
  let rescanning = $state(false);

  // Rules/fallback live in prefs; a missing map is treated as no rules.
  const ideRules = $derived(prefs.ideRules ?? {});
  const ideFallback = $derived(prefs.ideFallback ?? ides[0]?.id ?? "");
  // Editors the user located by executable path — shown as a removable list so a
  // stale entry can be dropped from every menu.
  const addedEditors = $derived(prefs.addedEditors ?? []);

  // The detected editor behind an id — undefined when the rule points at an
  // editor that's no longer installed (the trigger then reads "Choose…", no icon).
  function detectedEditor(editorId: string): Ide | undefined {
    return ides.find(editor => editor.id === editorId);
  }
  // The detected editors that suit a kind, in the backend's priority order. An
  // unknown kind (no entry) falls back to every editor rather than hiding them all.
  function editorsForKind(kind: string): Ide[] {
    const ids = kindOptions[kind];
    if (!ids) {
      return ides;
    }

    return ids
      .map(id => ides.find(editor => editor.id === id))
      .filter((editor): editor is Ide => editor !== undefined);
  }
  // Stable, valid popover id/anchor per editor select (kind or "fallback").
  function editorSelectId(key: string): string {
    return `ide-${key.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }
</script>

{#snippet editorSelect({ key, value, options, onpick, ariaLabel }: {
  key: string;
  value: string;
  options: Ide[];
  onpick: (editorId: string) => void;
  ariaLabel: string;
})}
  {@const selectId = editorSelectId(key)}
  {@const pickedEditor = detectedEditor(value)}
  <span class="editor-selector menu-host">
    <button
      style:anchor-name="--{selectId}"
      class="editor-trigger menu-trigger"
      aria-label={ariaLabel}
      disabled={ides.length === 0}
      popovertarget={selectId}
      type="button"
    >
      {#if pickedEditor}
        <span class="editor-icon" aria-hidden="true" data-brand={ideBrand(pickedEditor.id)}>
          <Icon name={ideIcon(pickedEditor.id)} size={15} />
        </span>
      {/if}
      <span>{pickedEditor?.label ?? "Choose…"}</span>
      <span class="caret" aria-hidden="true">▾</span>
    </button>
    <ul id={selectId} style:position-anchor="--{selectId}" class="menu editor-menu popover-menu" popover>
      {#each options as editor (editor.id)}
        {@const isPicked = editor.id === value}
        <li>
          <button
            class="menu-item editor-option"
            class:picked={isPicked}
            aria-current={isPicked}
            onclick={() => onpick(editor.id)}
            popovertarget={selectId}
            popovertargetaction="hide"
            type="button"
          >
            <span class="option-label">
              <span class="editor-icon" aria-hidden="true" data-brand={ideBrand(editor.id)}>
                <Icon name={ideIcon(editor.id)} size={15} />
              </span>
              <span>{editor.label}</span>
            </span>
            {#if isPicked}
              <span class="tick" aria-hidden="true">✓</span>
            {/if}
          </button>
        </li>
      {:else}
        <li class="none editor-empty">No editors detected.</li>
      {/each}
    </ul>
  </span>
{/snippet}

<section class="editors">
  <div class="editor-header">
    <div class="editor-header-copy">
      <h2>Editors</h2>
      <p class="hint">
        PADE reads what’s in a folder and opens it in the editor you set for
        that kind of project. Rules win over order — no shuffling a priority
        list.
      </p>
    </div>
    <button
      class="rescan"
      class:scanning={rescanning}
      aria-label="Re-detect installed editors"
      data-tooltip="Re-detect installed editors"
      onclick={async () => {
        if (rescanning) {
          return;
        }

        rescanning = true;
        try {
          const found = await onrescan();
          showToast(`Re-detected ${found} editor${found === 1 ? "" : "s"} on this machine`);
        } finally {
          rescanning = false;
        }
      }}
      type="button"
    ><Icon name="refresh" size={14} /> {#if rescanning}
      Detecting…{:else}Reload{/if}</button>
  </div>
  <ul class="editor-rules">
    {#each kinds as { kind, label, signals } (kind)}
      <li class="editor-rule">
        <span class="editor-kind">
          <span class="editor-label-row">
            <span class="kind-logo" aria-hidden="true" data-brand={languageIcon(kind)}>
              <Icon name={languageIcon(kind)} size={15} />
            </span>
            <span class="editor-label">{label}</span>
          </span>
          <span class="editor-signals">
            {#each signals as sig (sig)}
              <code class="sig">{sig}</code>
            {/each}
          </span>
        </span>
        <span class="editor-open">
          <span class="editor-arrow">detected → open in</span>
          {@render editorSelect({
            key: kind,
            value: ideRules[kind] ?? ideFallback,
            options: editorsForKind(kind),
            onpick: editorId => onrule({
              kind,
              editorId
            }),
            ariaLabel: `Editor for ${label} projects`
          })}
        </span>
      </li>
    {/each}
    <li class="editor-rule fallback">
      <!-- The catch-all: a folder matching no single-language rule — including a
           polyglot monorepo no one editor fully covers — carries a folder logo
           rather than any one language's mark. -->
      <span class="editor-label-row">
        <span class="kind-logo" aria-hidden="true">
          <Icon name="folder" size={15} />
        </span>
        <span class="editor-label">Any other folder</span>
      </span>
      <span class="editor-open">
        <span class="editor-arrow">fall back to</span>
        {@render editorSelect({
          key: "fallback",
          value: ideFallback,
          options: editorsForKind("fallback"),
          onpick: onfallback,
          ariaLabel: "Fallback editor"
        })}
      </span>
    </li>
  </ul>

  <!-- Locate an editor PADE didn't auto-detect on PATH. -->
  <div class="editor-addition">
    <div class="editor-addition-header">
      <span class="editor-addition-icon" aria-hidden="true"><Icon name="monitor" /></span>
      <span class="editor-addition-copy">
        <strong>Using an editor that isn’t listed?</strong>
        <small>
          PADE lists the editors it found automatically. Point it at any other
          editor’s executable and it’ll appear in the menus above.
        </small>
      </span>
      {#if !adding}
        <button
          class="editor-addition-button"
          onclick={() => {
            adding = true;
            draft = "";
            status = null;
          }}
          type="button"
        >
          <Icon name="plus" /> <span>Add editor…</span>
        </button>
      {/if}
    </div>

    {#if adding}
      <form
        class="editor-addition-form"
        onsubmit={async e => {
          e.preventDefault();
          // The path is a trust boundary — trim/length-cap it before it leaves the UI.
          const path = parseInput({
            schema: FolderPath,
            raw: draft
          });
          if (path === null) {
            status = {
              kind: StatusKind.Error,
              text: "Enter the full path to an editor executable."
            };
            return;
          }

          const result = await onaddeditor(path);
          if ("error" in result) {
            status = {
              kind: StatusKind.Error,
              text: result.error
            };
            return;
          }

          status = {
            kind: StatusKind.Ok,
            text: `${result.label} added.`
          };
          showToast(`${result.label} added`);
          adding = false;
          draft = "";
        }}
      >
        <div class="editor-location">
          <span class="editor-location-icon" aria-hidden="true"><Icon name="folder" /></span>
          <label class="visually-hidden" for="editor-location-input">Path to editor executable</label>
          <input
            id="editor-location-input"
            class="editor-location-input"
            autocomplete="off"
            oninput={() => {
              // Clear a stale message the moment the user edits the path.
              status = null;
            }}
            placeholder="C:\path\to\editor.exe"
            spellcheck="false"
            bind:value={draft}
          />
          <button
            class="editor-browse"
            onclick={async () => {
              const picked = await openDialog({
                multiple: false,
                title: "Locate an editor’s executable"
              });
              if (typeof picked === "string") {
                draft = picked;
                status = null;
              }
            }}
            type="button"
          >Browse…</button>
        </div>
        <div class="editor-addition-actions">
          <button class="editor-confirm" type="submit">Add editor</button>
          <button
            class="editor-cancel"
            onclick={() => {
              adding = false;
              draft = "";
              status = null;
            }}
            type="button"
          >Cancel</button>
          <span class="editor-addition-hint">Select the editor’s executable</span>
        </div>
      </form>
    {/if}

    {#if status}
      <output class="editor-status" class:error={status.kind === StatusKind.Error}>
        <span class="editor-status-icon" aria-hidden="true">
          <Icon name={status.kind === StatusKind.Ok ? "check" : "alert"} size={14} />
        </span>
        <span>{status.text}</span>
      </output>
    {/if}

    <!-- Editors located by executable path — listed so a stale one can be dropped. -->
    {#if addedEditors.length > 0}
      <div class="editor-added">
        <span class="editor-added-eyebrow">Added manually</span>
        {#each addedEditors as editor (editor.id)}
          <div class="editor-added-row">
            <span class="editor-added-information">
              <span class="editor-added-name-row">
                <span class="editor-added-name">{editor.label}</span>
                <span class="editor-added-tag">added</span>
              </span>
              <span class="editor-added-path" data-tooltip={editor.path}>{editor.path}</span>
            </span>
            <button
              class="editor-remove"
              aria-label={`Remove ${editor.label}`}
              data-tooltip="Remove"
              onclick={async () => {
                await onremoveeditor(editor.id);
                showToast(`Removed ${editor.label}`);
              }}
              type="button"
            ><Icon name="trash" size={15} /></button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  /* ── Editor-rules engine — one tonal row per project kind, plus a dashed
     fall-back row. Each row carries a native-popover editor select. ── */
  .editor-header {
    display: flex;
    gap: 12px;
    justify-content: space-between;
    align-items: flex-start;
  }

  .editor-header-copy {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-inline-size: 0;

    .hint {
      max-inline-size: 62ch;
      font-size: 12px;
      line-height: 1.5;
    }
  }

  .editor-rules {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .editor-rule {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    justify-content: space-between;
    align-items: center;
    padding-block: 10px;
    padding-inline: 14px 8px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
    transition:
      border-color 150ms var(--ease),
      background 150ms var(--ease);

    /* Keep the row active while its editor menu is open — the pointer has moved
       onto the top-layer popover, so :hover alone would drop off the row. */
    &:hover,
    &:has(.editor-menu:popover-open) {
      border-color: var(--primary-container);
      background: var(--surface-2);
    }

    &.fallback {
      border-style: dashed;
    }
  }

  /* The kind block flexes and shrinks so many signal chips wrap WITHIN it —
     never pushing the editor select onto its own line below. */
  .editor-kind {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 6px;
    min-inline-size: min(150px, 100%);
  }

  .editor-label-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  /* Language logo leading the kind label — muted so the panel stays calm. */

  /* The language's own colour (theme.css [data-brand]); an officially-black
     brand (Rust) has no tint and falls back to the muted text colour. */
  .kind-logo {
    display: inline-flex;
    flex: none;
    color: var(--brand-color, var(--on-surface-variant));
  }

  .editor-label {
    font-weight: 600;
    font-size: 13px;
  }

  /* Per-kind manifest signals — small mono surface-3 chips. */
  .editor-signals {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .sig {
    padding: 2px 6px;
    border-radius: var(--radius-small);
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 10px;
  }

  /* The "detected → open in" label and the select move as one unit, so a
     narrow panel wraps them together instead of stranding the label. */
  .editor-open {
    display: flex;
    flex: none;
    gap: 12px;
    align-items: center;
  }

  .editor-arrow {
    flex: none;
    color: var(--on-surface-variant);
    font-size: 12px;
  }

  .editor-selector {
    position: relative;
    flex: none;
  }

  /* Popover select trigger — pill that brightens its edge on hover. */
  .editor-trigger {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding: 8px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition:
      border-color 150ms var(--ease),
      background 150ms var(--ease);

    &:hover:not(:disabled) {
      border-color: var(--primary);
      background: var(--surface-3);
      filter: none;
    }

    .caret {
      display: inline-block;
      font-size: 9px;
      opacity: 70%;
      transition: rotate 150ms var(--ease);
    }
  }

  /* Editor brand mark in the trigger and each option. */

  /* The editor's brand colour; a black brand (JetBrains) follows the text. */
  .editor-icon {
    display: inline-flex;
    flex: none;
    color: var(--brand-color, currentColor);
  }

  /* Reuse the row-menu popover chrome; align + size for a select. */
  .editor-menu {
    min-inline-size: 180px;

    .editor-option {
      justify-content: space-between;
      font-weight: 600;
    }

    .option-label {
      display: inline-flex;
      gap: 8px;
      align-items: center;
    }
  }

  .editor-empty {
    padding: 8px 10px;
  }

  /* ── "Add editor…" — locate an editor PADE didn't find on PATH. ── */
  .editor-addition {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
  }

  .editor-addition-header {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }

  .editor-addition-icon {
    display: inline-flex;
    flex: none;
    color: var(--primary);
  }

  .editor-addition-copy {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-inline-size: 0;

    strong {
      font-weight: 700;
      font-size: 13px;
    }

    small {
      color: var(--on-surface-variant);
      font-size: 12px;
      line-height: 1.45;
    }
  }

  /* Reveal button — tonal surface-3 pill that fills primary-container on hover. */
  .editor-addition-button {
    display: inline-flex;
    flex: none;
    gap: 6px;
    align-items: center;
    padding: 7px 12px;
    border: none;
    border-radius: var(--radius-small);
    background: var(--surface-3);
    color: var(--on-surface);
    font: inherit;
    font-weight: 700;
    font-size: 12px;
    cursor: pointer;
    transition:
      background 150ms var(--ease),
      color 150ms var(--ease);

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .editor-addition-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Path field — mono input with a folder lead and a ghost Browse…, primary edge. */
  .editor-location {
    display: flex;
    gap: 4px;
    align-items: center;
    padding-block: 3px;
    padding-inline: 10px 3px;
    border: 1px solid var(--primary);
    border-radius: var(--radius-medium);
    background: var(--surface-2);

    .editor-location-icon {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }
  }

  .editor-location-input {
    flex: 1;
    min-inline-size: 0;
    padding: 6px 4px;
    border: none;
    background: transparent;
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 0.75rem;
  }

  .editor-browse {
    flex: none;
    padding: 6px 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-weight: 600;
    font-size: 0.6875rem;
    cursor: pointer;
    transition:
      background 150ms var(--ease),
      color 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }

  .editor-addition-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .editor-addition-hint {
    margin-inline-start: auto;
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .editor-confirm {
    padding: 7px 14px;
    border: none;
    border-radius: var(--radius-small);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 700;
    font-size: 12px;
    cursor: pointer;
    transition: filter 150ms var(--ease);

    &:hover {
      filter: brightness(1.06);
    }
  }

  .editor-cancel {
    padding: 8px 14px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }
  }

  /* Inline result — tertiary wash on success, crit wash on rejection. */
  .editor-status {
    display: flex;
    gap: 7px;
    align-items: flex-start;
    padding: 7px 9px;
    border-radius: var(--radius-small);
    background: var(--tertiary-wash);
    color: var(--tertiary);
    font-size: 11px;
    line-height: 1.45;
    animation: line-in 180ms var(--ease);

    &.error {
      background: var(--critical-wash);
      color: var(--critical);
    }

    .editor-status-icon {
      display: inline-flex;
      flex: none;
      margin-block-start: 1px;
    }
  }

  /* ── "Added manually" — editors located by path, listed for removal. ── */
  .editor-added {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .editor-added-eyebrow {
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .editor-added-row {
    display: flex;
    gap: 10px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 12px 8px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
  }

  .editor-added-information {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-inline-size: 0;
  }

  .editor-added-name-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .editor-added-name {
    font-weight: 600;
    font-size: 13px;
  }

  /* "added" provenance tag — a quiet tertiary micro-label, no fill. */
  .editor-added-tag {
    flex: none;
    color: var(--tertiary);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .editor-added-path {
    overflow: hidden;
    max-inline-size: 100%;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Remove — a circle that reddens on hover, like the roots-row close button. */
  .editor-remove {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 28px;
    inline-size: 28px;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition:
      background 150ms var(--ease),
      color 150ms var(--ease);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
      filter: none;
    }
  }
</style>
