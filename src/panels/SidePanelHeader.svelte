<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { panelCount, panelRefresh } from "@/lib/stores/ui/sidePanel.svelte";
  import { formatCount } from "@/lib/util/format";

  // One shared header for every side panel (DRY): title + the optional count and
  // refresh the open panel publishes through the sidePanel store.
  const { title }: { title: string } = $props();

  const count = $derived(panelCount());
  const refresh = $derived(panelRefresh());
</script>

<header class="panel-head">
  <div class="panel-title">
    <h2>{title}</h2>
    {#if count !== null}
      <span class="panel-count">{formatCount(count)}</span>
    {/if}
  </div>
  {#if refresh}
    <button
      class="panel-refresh"
      aria-label="Refresh"
      data-tooltip="Refresh"
      onclick={() => refresh()}
    >
      <Icon name="refresh" />
    </button>
  {/if}
</header>

<style>
  /* Every header is the same height whether or not its panel publishes a
     refresh action: the title row reserves a control's worth of space, so Tasks
     and Git (which have the button) no longer stand 8px taller than Feed and
     Config (which don't), and switching panels never nudges the body down. */
  .panel-head {
    --panel-head-control-size: 28px;

    display: flex;
    flex-shrink: 0;
    gap: 8px;
    align-items: center;
    padding-block: 12px 10px;
    padding-inline: 16px;

    /* Cyberpunk: one of the HUD's brightest lines (with the top bar and the
       side panel's seam), led by the console's signature 44px accent stub -
       the mark that says "this is a titled section" without spending a colour
       on the title itself. */
    :global(:root[data-palette="cyberpunk"]) & {
      border-block-end: 2px solid var(--hairline);
      background-image: var(--header-stub);
      background-position: bottom left;
      background-size: 100% var(--header-stub-weight);
      background-repeat: no-repeat;
      box-shadow: var(--hairline-glow-below);
    }
  }

  .panel-title {
    display: flex;
    gap: 9px;
    align-items: center;
    min-block-size: var(--panel-head-control-size);

    h2 {
      margin: 0;
      font-weight: 700;
      font-size: 0.9375rem;
    }
  }

  .panel-count {
    padding: 2px 9px;
    border-radius: var(--radius-full);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 700;
    font-size: 0.75rem;
  }

  .panel-refresh {
    display: grid;
    place-items: center;
    block-size: var(--panel-head-control-size);
    inline-size: var(--panel-head-control-size);
    margin-inline-start: auto;
    border: none;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: color 140ms var(--ease), background 140ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }
</style>
