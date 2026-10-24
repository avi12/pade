<script lang="ts">
  import Icon from "@/lib/Icon.svelte";

  // The save card's destinations: one Save row per saved root, or - with no
  // roots yet - a single row that picks a folder to become the first root and
  // saves into it. The card owns every gate; a row only asks it.
  const { roots, isCreateEnabled, isNameTakenIn, canSaveInto, oncreateroot, onsaveinto }: {
    roots: string[];
    /** The no-roots row can save - a valid name and nothing in flight. */
    isCreateEnabled: boolean;
    /** The typed name already exists in this root. */
    isNameTakenIn: (root: string) => boolean;
    /** This root's Save is open - its button's enabled state. */
    canSaveInto: (root: string) => boolean;
    /** Pick a folder as the first root, then save into it. */
    oncreateroot: () => void;
    onsaveinto: (root: string) => void;
  } = $props();

  function saveGoLabel(isTaken: boolean) {
    if (isTaken) {
      return "Name taken";
    }

    return "Save →";
  }
</script>

<!-- display: contents - present only so the rows' rules stay nested under it. -->
<div class="save-destinations">
  {#if roots.length === 0}
    <button
      class="save-root"
      disabled={!isCreateEnabled}
      onclick={oncreateroot}
      type="button"
    >
      <span class="save-root-icon" aria-hidden="true"><Icon name="folderPlus" size={15} /></span>
      <span class="save-root-path">Choose a folder for your projects…</span>
      <span class="save-go">Create root & save →</span>
    </button>
  {:else}
    <div class="save-roots">
      {#each roots as root (root)}
        {@const isTaken = isNameTakenIn(root)}
        <button
          class="save-root"
          class:save-root-collides={isTaken}
          disabled={!canSaveInto(root)}
          onclick={() => onsaveinto(root)}
          type="button"
        >
          <span class="save-root-icon" aria-hidden="true"><Icon name="folder" size={15} /></span>
          <span class="save-root-path">{root}</span>
          <span class="save-go">{saveGoLabel(isTaken)}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .save-destinations {
    display: contents;

    .save-roots {
      display: flex;
      flex-direction: column;
      gap: 3px;
    }

    .save-root {
      display: flex;
      gap: 8px;
      align-items: center;
      inline-size: 100%;
      padding-block: 7px;
      padding-inline: 8px;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      text-align: start;
      cursor: pointer;
      transition: color 120ms var(--ease), background 120ms var(--ease);

      &:hover:not(:disabled) {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }

      &:disabled {
        opacity: 55%;
        cursor: default;
      }

      .save-root-icon {
        display: inline-flex;
        flex: none;
        color: var(--on-surface-variant);
      }

      .save-root-path {
        flex: 1;
        overflow: hidden;
        min-inline-size: 0;
        font-family: var(--font-monospace);
        font-weight: 600;
        font-size: 0.6875rem;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .save-go {
        flex: none;
        color: var(--primary);
        font-weight: 700;
        font-size: 0.625rem;
      }

      &.save-root-collides .save-go {
        color: var(--critical);
      }

      &:hover:not(:disabled) .save-go {
        color: var(--on-primary-container);
      }
    }
  }
</style>
