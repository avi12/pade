<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";

  // The terminal font, picked from two-column cards that each preview their face.
  // `value` is the persisted monoFont string ("" = system default); `family` is
  // the CSS stack the preview renders in that face.
  const fontOptions = [
    {
      name: "JetBrains Mono",
      value: "JetBrains Mono",
      family: "\"JetBrains Mono\", ui-monospace, monospace"
    },
    {
      name: "System Mono",
      value: "",
      family: "ui-monospace, monospace"
    },
    {
      name: "Courier",
      value: "Courier New",
      family: "\"Courier New\", monospace"
    },
    {
      name: "Fira Code",
      value: "Fira Code",
      family: "\"Fira Code\", ui-monospace, monospace"
    },
    {
      name: "Share Tech Mono",
      value: "Share Tech Mono",
      family: "\"Share Tech Mono\", ui-monospace, monospace"
    }
  ] as const satisfies readonly {
    name: string;
    value: string;
    family: string;
  }[];

  const selectedMonospaceFont = $derived(effective.monoFontName);
</script>

<div class="field">
  <span class="field-label">Terminal font</span>
  <div class="font-grid">
    {#each fontOptions as option (option.name)}
      <button
        class="font-card"
        class:on={selectedMonospaceFont === option.value}
        aria-pressed={selectedMonospaceFont === option.value}
        onclick={() => updatePrefs({ monoFont: option.value })}
      >
        <span class="font-text">
          <span class="font-name">{option.name}</span>
          <span style:font-family={option.family} class="font-preview">Ag0 &lt;/&gt; 1l</span>
        </span>
        {#if selectedMonospaceFont === option.value}
          <Icon name="check" />
        {/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .font-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .font-card {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
    padding: 8px 10px;
    border: 1px solid var(--outline);
    border-radius: 10px;
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition:
      background 140ms var(--ease),
      border-color 140ms var(--ease);

    &:hover:not(.on) {
      background: var(--surface-2);
    }

    &.on {
      border-color: var(--primary);
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    .font-text {
      display: flex;
      flex-direction: column;
      gap: 1px;
      min-inline-size: 0;
      line-height: 1.3;
    }

    .font-name {
      overflow: hidden;
      font-weight: 600;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .font-preview {
      font-size: 11px;
      opacity: 70%;
    }
  }
</style>
