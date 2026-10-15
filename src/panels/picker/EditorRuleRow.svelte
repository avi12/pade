<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import type { Ide } from "@/lib/types";
  import EditorSelect from "@/panels/picker/EditorSelect.svelte";

  // One row of the editor-rules engine: a project kind (logo, label and the
  // manifest signals PADE looks for) mapped to the editor it opens in. With no
  // `signals` it is the dashed catch-all fall-back row, which carries no kind
  // block - just its label.
  const { selectKey, label, icon, brand, signals, arrow, value, options, ides, onpick, ariaLabel }: {
    /** The select's key - the kind, or "fallback". */
    selectKey: string;
    label: string;
    icon: IconName;
    /** The language's brand for the logo's colour (theme.css [data-brand]). */
    brand?: string;
    /** The kind's manifest signals, or null for the fall-back row. */
    signals: string[] | null;
    /** The words leading the select ("detected → open in"). */
    arrow: string;
    value: string;
    options: Ide[];
    ides: Ide[];
    onpick: (editorId: string) => void;
    ariaLabel: string;
  } = $props();

  const isFallback = $derived(signals === null);
</script>

{#snippet labelRow()}
  <span class="editor-label-row">
    <span class="kind-logo" aria-hidden="true" data-brand={brand}>
      <Icon name={icon} size={15} />
    </span>
    <span class="editor-label">{label}</span>
  </span>
{/snippet}

<li class="editor-rule" class:fallback={isFallback}>
  {#if signals}
    <span class="editor-kind">
      {@render labelRow()}
      <span class="editor-signals">
        {#each signals as sig (sig)}
          <code class="sig">{sig}</code>
        {/each}
      </span>
    </span>
  {:else}
    {@render labelRow()}
  {/if}
  <span class="editor-open">
    <span class="editor-arrow">{arrow}</span>
    <EditorSelect {ariaLabel} {ides} key={selectKey} {onpick} {options} {value} />
  </span>
</li>

<style>
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

    /* Keep the row active while its editor menu is open - the pointer has moved
       onto the top-layer popover, so :hover alone would drop off the row. The
       menu lives in the child EditorSelect, so `:global` reaches it - still
       bounded by this row's scoped selector. */
    &:hover,
    &:has(:global(.editor-menu:popover-open)) {
      border-color: var(--primary-container);
      background: var(--surface-2);
    }

    &.fallback {
      border-style: dashed;
    }
  }

  /* The kind block flexes and shrinks so many signal chips wrap WITHIN it -
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

  /* Language logo leading the kind label - muted so the panel stays calm. */

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

  /* Per-kind manifest signals - small mono surface-3 chips. */
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
</style>
