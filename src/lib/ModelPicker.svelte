<script lang="ts">
  // An agent's models as a pick list, from its own catalog: the one in use
  // checked, each with the effort level the agent recommends for it.

  import type { ModelOption } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";

  const { options, current, onpick }: {
    options: ModelOption[];
    /** The model in use (or picked), checked. */
    current: string | null;
    onpick: (model: string) => Promise<void>;
  } = $props();
</script>

<ul class="models">
  {#each options as option (option.id)}
    {@const isCurrent = current === option.id}
    <li>
      <button
        class="model-row"
        class:current={isCurrent}
        aria-pressed={isCurrent}
        onclick={async () => await onpick(option.id)}
      >
        <span class="check">
          {#if isCurrent}
            <Icon name="check" size={12} />
          {/if}
        </span>
        <span class="name">{option.name}</span>
        <span class="hint">{option.recommendedEffort ?? ""}</span>
      </button>
    </li>
  {/each}
</ul>

<style>
  .models {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .model-row {
    display: flex;
    gap: 8px;
    align-items: center;
    inline-size: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-size: 13px;
    text-align: start;
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
    }

    &.current {
      color: var(--primary);
      font-weight: 600;
    }

    .check {
      display: inline-flex;
      flex: none;
      inline-size: 12px;
    }

    .name {
      flex: 1;
    }

    .hint {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 10px;
    }
  }
</style>
