<script lang="ts">
  // The Config panel's way to the settings that live on the project picker
  // (default agent, editors, root folders, what a launch does): a card that
  // shows the picker, leaving this window's sessions running behind it.
  const { onopen }: {
    onopen: () => void;
  } = $props();
</script>

<button class="setup-link" onclick={onopen} type="button">
  <span class="text">
    <span class="title">Agents, editors & folders</span>
    <span class="hint">Default agent, detected editors, root folders, launch behavior</span>
  </span>
  <span class="arrow" aria-hidden="true">→</span>
</button>

<style>
  /* Dressed as a settings card (the same outline, corners and surface tokens
     SettingsCard wears), so each palette reshapes it with the cards. */
  .setup-link {
    display: flex;
    gap: 10px;
    align-items: center;
    padding-block: 12px;
    padding-inline: 14px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: border-color 140ms var(--ease);

    &:hover {
      border-color: var(--primary);
    }
  }

  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    line-height: 1.3;
  }

  .title {
    font-weight: 700;
    font-size: 0.8125rem;
  }

  .hint {
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .arrow {
    color: var(--on-surface-variant);
  }
</style>
