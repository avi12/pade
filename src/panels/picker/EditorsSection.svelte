<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { ideBrand, ideIcon } from "@/lib/ideIcon";
  import { languageIcon } from "@/lib/languageIcon";
  import { showToast } from "@/lib/stores/toast.svelte";
  import type { EditorKind, Ide, Prefs } from "@/lib/types";
  import { FolderPath, parseInput } from "@/lib/validate";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  // Editor-rules engine — the project kinds come from the backend registry
  // (`ide_kinds`, priority order), one row per kind. A rule maps a kind to an
  // editor id; unmatched folders use the fallback. Each kind carries the
  // manifest files PADE looks for to classify a folder.
  // Rule/fallback persistence stays with the parent (single settings owner);
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
  <span class="editor-sel menu-host">
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
            class="mi editor-opt"
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
  <div class="ed-head">
    <div class="ed-head-copy">
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
  <ul class="ed-rules">
    {#each kinds as { kind, label, signals } (kind)}
      <li class="ed-rule">
        <span class="ed-kind">
          <span class="ed-label-row">
            <span class="kind-logo" aria-hidden="true" data-brand={languageIcon(kind)}>
              <Icon name={languageIcon(kind)} size={15} />
            </span>
            <span class="ed-label">{label}</span>
          </span>
          <span class="ed-signals">
            {#each signals as sig (sig)}
              <code class="sig">{sig}</code>
            {/each}
          </span>
        </span>
        <span class="ed-spacer"></span>
        <span class="ed-arrow">detected → open in</span>
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
      </li>
    {/each}
    <li class="ed-rule fallback">
      <!-- The catch-all: a folder matching no single-language rule — including a
           polyglot monorepo no one editor fully covers — carries a folder logo
           rather than any one language's mark. -->
      <span class="ed-label-row">
        <span class="kind-logo" aria-hidden="true">
          <Icon name="folder" size={15} />
        </span>
        <span class="ed-label">Any other folder</span>
      </span>
      <span class="ed-spacer"></span>
      <span class="ed-arrow">fall back to</span>
      {@render editorSelect({
        key: "fallback",
        value: ideFallback,
        options: editorsForKind("fallback"),
        onpick: onfallback,
        ariaLabel: "Fallback editor"
      })}
    </li>
  </ul>

  <!-- Locate an editor PADE didn't auto-detect on PATH. -->
  <div class="ed-add">
    <div class="ed-add-head">
      <span class="ed-add-ico" aria-hidden="true"><Icon name="monitor" /></span>
      <span class="ed-add-copy">
        <strong>Using an editor that isn’t listed?</strong>
        <small>
          PADE lists the editors it found automatically. Point it at any other
          editor’s executable and it’ll appear in the menus above.
        </small>
      </span>
      {#if !adding}
        <button
          class="ed-add-btn"
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
        class="ed-add-form"
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
        <div class="ed-locate">
          <span class="ed-locate-ico" aria-hidden="true"><Icon name="folder" /></span>
          <label class="visually-hidden" for="ed-locate-input">Path to editor executable</label>
          <input
            id="ed-locate-input"
            class="ed-locate-input"
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
            class="ed-browse"
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
        <div class="ed-add-actions">
          <button class="ed-confirm" type="submit">Add editor</button>
          <button
            class="ed-cancel"
            onclick={() => {
              adding = false;
              draft = "";
              status = null;
            }}
            type="button"
          >Cancel</button>
          <span class="ed-add-hint">Select the editor’s executable</span>
        </div>
      </form>
    {/if}

    {#if status}
      <output class="ed-status" class:err={status.kind === StatusKind.Error}>
        <span class="ed-status-ico" aria-hidden="true">
          <Icon name={status.kind === StatusKind.Ok ? "check" : "alert"} size={14} />
        </span>
        <span>{status.text}</span>
      </output>
    {/if}

    <!-- Editors located by executable path — listed so a stale one can be dropped. -->
    {#if addedEditors.length > 0}
      <div class="ed-added">
        <span class="ed-added-eyebrow">Added manually</span>
        {#each addedEditors as editor (editor.id)}
          <div class="ed-added-row">
            <span class="ed-added-info">
              <span class="ed-added-name-row">
                <span class="ed-added-name">{editor.label}</span>
                <span class="ed-added-tag">added</span>
              </span>
              <span class="ed-added-path" data-tooltip={editor.path}>{editor.path}</span>
            </span>
            <button
              class="ed-remove"
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
  .ed-head {
    display: flex;
    gap: 12px;
    justify-content: space-between;
    align-items: flex-start;
  }

  .ed-head-copy {
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

  .ed-rules {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .ed-rule {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
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

  .ed-kind {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-inline-size: 150px;
  }

  .ed-label-row {
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

  .ed-label {
    font-weight: 600;
    font-size: 13px;
  }

  /* Per-kind manifest signals — small mono surface-3 chips. */
  .ed-signals {
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

  .ed-spacer {
    flex: 1;
    min-inline-size: 8px;
  }

  .ed-arrow {
    flex: none;
    color: var(--on-surface-variant);
    font-size: 12px;
  }

  .editor-sel {
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

    .editor-opt {
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
  .ed-add {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
  }

  .ed-add-head {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }

  .ed-add-ico {
    display: inline-flex;
    flex: none;
    color: var(--primary);
  }

  .ed-add-copy {
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
  .ed-add-btn {
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

  .ed-add-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Path field — mono input with a folder lead and a ghost Browse…, primary edge. */
  .ed-locate {
    display: flex;
    gap: 4px;
    align-items: center;
    padding-block: 3px;
    padding-inline: 10px 3px;
    border: 1px solid var(--primary);
    border-radius: var(--radius-medium);
    background: var(--surface-2);

    .ed-locate-ico {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }
  }

  .ed-locate-input {
    flex: 1;
    min-inline-size: 0;
    padding: 6px 4px;
    border: none;
    background: transparent;
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 0.75rem;
  }

  .ed-browse {
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

  .ed-add-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .ed-add-hint {
    margin-inline-start: auto;
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .ed-confirm {
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

  .ed-cancel {
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
  .ed-status {
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

    &.err {
      background: var(--critical-wash);
      color: var(--critical);
    }

    .ed-status-ico {
      display: inline-flex;
      flex: none;
      margin-block-start: 1px;
    }
  }

  /* ── "Added manually" — editors located by path, listed for removal. ── */
  .ed-added {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .ed-added-eyebrow {
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .ed-added-row {
    display: flex;
    gap: 10px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 12px 8px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
  }

  .ed-added-info {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-inline-size: 0;
  }

  .ed-added-name-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .ed-added-name {
    font-weight: 600;
    font-size: 13px;
  }

  /* "added" provenance tag — a quiet tertiary micro-label, no fill. */
  .ed-added-tag {
    flex: none;
    color: var(--tertiary);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .ed-added-path {
    overflow: hidden;
    max-inline-size: 100%;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Remove — a circle that reddens on hover, like the roots-row close button. */
  .ed-remove {
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
