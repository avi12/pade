<script lang="ts">
  import type { Ide, Prefs } from "@/lib/types";

  // Editor-rules engine — fixed, priority-ordered project kinds. A rule maps a
  // kind to an editor id; unmatched folders use the fallback. One row per kind.
  // Each kind carries the manifest files PADE looks for to classify a folder.
  // Rule/fallback persistence stays with the parent (single settings owner);
  // this section renders the rows and reports picks.
  const {
    ides,
    currentKind,
    prefs,
    onrule,
    onfallback
  }: {
    ides: Ide[];
    /** Primary detected kind of the current dir — tags "this project"'s row. */
    currentKind: string | null;
    prefs: Prefs;
    onrule: (rule: {
      kind: string;
      editorId: string;
    }) => void;
    onfallback: (editorId: string) => void;
  } = $props();

  const EDITOR_KINDS = [
    {
      kind: "web",
      label: "Web / JavaScript",
      signals: ["package.json"]
    },
    {
      kind: "python",
      label: "Python",
      signals: ["pyproject.toml", "requirements.txt"]
    },
    {
      kind: "java",
      label: "Java",
      signals: ["pom.xml", "build.gradle"]
    },
    {
      kind: "go",
      label: "Go",
      signals: ["go.mod"]
    },
    {
      kind: "rust",
      label: "Rust",
      signals: ["Cargo.toml"]
    },
    {
      kind: "android",
      label: "Android",
      signals: ["build.gradle", "AndroidManifest.xml"]
    }
  ] as const;

  // Rules/fallback live in prefs; a missing map is treated as no rules.
  const ideRules = $derived(prefs.ideRules ?? {});
  const ideFallback = $derived(prefs.ideFallback ?? ides[0]?.id ?? "");

  function editorLabel(editorId: string): string {
    return ides.find(editor => editor.id === editorId)?.label ?? "Choose…";
  }
  // Stable, valid popover id/anchor per editor select (kind or "fallback").
  function editorSelectId(key: string): string {
    return `ide-${key.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }
</script>

{#snippet editorSelect({ key, value, onpick, ariaLabel }: {
  key: string;
  value: string;
  onpick: (editorId: string) => void;
  ariaLabel: string;
})}
  {@const selectId = editorSelectId(key)}
  <span class="editor-sel">
    <button
      style:anchor-name="--{selectId}"
      class="editor-trigger"
      aria-label={ariaLabel}
      disabled={ides.length === 0}
      popovertarget={selectId}
      type="button"
    >
      <span>{editorLabel(value)}</span>
      <span class="caret" aria-hidden="true">▾</span>
    </button>
    <ul id={selectId} style:position-anchor="--{selectId}" class="menu editor-menu" popover>
      {#each ides as editor (editor.id)}
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
            <span>{editor.label}</span>
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
    <h2>Editors</h2>
    <p class="hint">
      PADE reads what’s in a folder and opens it in the editor you set for
      that kind of project. Rules win over order — no shuffling a priority
      list.
    </p>
  </div>
  <ul class="ed-rules">
    {#each EDITOR_KINDS as { kind, label, signals } (kind)}
      {@const isThisProject = currentKind === kind}
      <li class="ed-rule" class:here={isThisProject}>
        <span class="ed-kind">
          <span class="ed-label-row">
            <span class="ed-label">{label}</span>
            {#if isThisProject}
              <span class="here-tag">this project</span>
            {/if}
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
          onpick: editorId => onrule({
            kind,
            editorId
          }),
          ariaLabel: `Editor for ${label} projects`
        })}
      </li>
    {/each}
    <li class="ed-rule fallback">
      <span class="ed-label">Any other folder</span>
      <span class="ed-spacer"></span>
      <span class="ed-arrow">fall back to</span>
      {@render editorSelect({
        key: "fallback",
        value: ideFallback,
        onpick: onfallback,
        ariaLabel: "Fallback editor"
      })}
    </li>
  </ul>
</section>

<style>
  /* ── Editor-rules engine — one tonal row per project kind, plus a dashed
     fall-back row. Each row carries a native-popover editor select. ── */
  .ed-head {
    display: flex;
    flex-direction: column;
    gap: 6px;

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

    &:hover {
      border-color: var(--primary-container);
      background: var(--surface-2);
    }

    &.fallback {
      border-style: dashed;
    }

    /* The current project's kind — a subtle tertiary edge. */
    &.here {
      border-color: var(--tertiary);
      background: var(--tertiary-wash);
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

  .here-tag {
    flex: none;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--tertiary);
    color: var(--on-primary);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
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
      font-size: 9px;
      opacity: 70%;
    }
  }

  /* Reuse the row-menu popover chrome; align + size for a select. */
  .editor-menu {
    min-inline-size: 180px;

    .editor-opt {
      justify-content: space-between;
      font-weight: 600;

      &.picked {
        color: var(--primary);
      }
    }
  }

  .editor-empty {
    padding: 8px 10px;
  }
</style>
