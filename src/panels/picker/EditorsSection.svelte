<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import type { EditorKind, Ide, Prefs } from "@/lib/types";
  import { formatQuantity } from "@/lib/util/format";
  import { languageIcon } from "@/lib/workspace/display/language-icon";
  import { editorsForKind } from "@/panels/picker/editor-options";
  import EditorAddition from "@/panels/picker/EditorAddition.svelte";
  import EditorRuleRow from "@/panels/picker/EditorRuleRow.svelte";

  // Editor-rules engine - the project kinds come from the backend registry
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
        from the backend registry in its render/priority order - the single home
        of the kind list, so a new kind needs no frontend change. */
    kinds: EditorKind[];
    /** Editor ids that suit each project kind (kind → ordered, installed-only), so
        a kind's menu offers only fitting editors - no WebStorm on an Android row. */
    kindOptions: Record<string, string[]>;
    prefs: Prefs;
    onrule: (rule: {
      kind: string;
      editorId: string;
    }) => void;
    onfallback: (editorId: string) => void;
    /** Add an editor by executable path - resolves to its label or a rejection. */
    onaddeditor: (path: string) => Promise<{
      label: string;
    } | {
      error: string;
    }>;
    /** Re-probe the machine for installed editors - resolves to how many were
        found, so the button can report the count. */
    onrescan: () => Promise<number>;
    /** Drop a user-added editor by id (removes it from every menu). */
    onremoveeditor: (id: string) => Promise<void>;
  } = $props();

  const FALLBACK_KEY = "fallback";

  // Re-detect in flight - flips the header's Reload button to "Detecting…".
  let isRescanning = $state(false);
  const rescanLabel = $derived.by(() => {
    if (isRescanning) {
      return "Detecting…";
    }

    return "Reload";
  });

  // Rules/fallback live in prefs; a missing map is treated as no rules.
  const ideRules = $derived(prefs.ideRules ?? {});
  const ideFallback = $derived(prefs.ideFallback ?? ides[0]?.id ?? "");
  // Editors the user located by executable path - shown as a removable list so a
  // stale entry can be dropped from every menu.
  const addedEditors = $derived(prefs.addedEditors ?? []);
  // Each kind's row: its rule (or the fallback) and the editors that suit it.
  const rules = $derived(
    kinds.map(entry => ({
      ...entry,
      value: ideRules[entry.kind] ?? ideFallback,
      options: editorsForKind({
        ides,
        kindOptions,
        kind: entry.kind
      })
    }))
  );
</script>

<section class="editors">
  <div class="editor-header">
    <div class="editor-header-copy">
      <h2>Editors</h2>
      <p class="hint">
        PADE reads what’s in a folder and opens it in the editor you set for
        that kind of project. Rules win over order - no shuffling a priority
        list.
      </p>
    </div>
    <button
      class="rescan"
      class:scanning={isRescanning}
      aria-label="Re-detect installed editors"
      data-tooltip="Re-detect installed editors"
      onclick={async () => {
        if (isRescanning) {
          return;
        }

        isRescanning = true;
        try {
          const found = await onrescan();
          showToast(
            `Re-detected ${formatQuantity({
              count: found,
              noun: "editor"
            })} on this machine`
          );
        } finally {
          isRescanning = false;
        }
      }}
      type="button"
    ><Icon name="refresh" size={14} /> {rescanLabel}</button>
  </div>
  <ul class="editor-rules">
    {#each rules as { kind, label, signals, value, options } (kind)}
      <EditorRuleRow
        ariaLabel={`Editor for ${label} projects`}
        arrow="detected → open in"
        brand={languageIcon(kind)}
        icon={languageIcon(kind)}
        {ides}
        {label}
        onpick={editorId => onrule({
          kind,
          editorId
        })}
        {options}
        selectKey={kind}
        {signals}
        {value}
      />
    {/each}
    <!-- The catch-all: a folder matching no single-language rule - including a
         polyglot monorepo no one editor fully covers - carries a folder logo
         rather than any one language's mark. -->
    <EditorRuleRow
      ariaLabel="Fallback editor"
      arrow="fall back to"
      icon="folder"
      {ides}
      label="Any other folder"
      onpick={onfallback}
      options={editorsForKind({
        ides,
        kindOptions,
        kind: FALLBACK_KEY
      })}
      selectKey={FALLBACK_KEY}
      signals={null}
      value={ideFallback}
    />
  </ul>

  <!-- Locate an editor PADE didn't auto-detect on PATH. -->
  <EditorAddition {addedEditors} {onaddeditor} {onremoveeditor} />
</section>

<style>
  /* ── Editor-rules engine - one tonal row per project kind, plus a dashed
     fall-back row (picker/EditorRuleRow.svelte). Each row carries a
     native-popover editor select (picker/EditorSelect.svelte). ── */
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
      font-size: 0.75rem;
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

  /* The "Add editor…" card lives in picker/EditorAddition.svelte. */
</style>
