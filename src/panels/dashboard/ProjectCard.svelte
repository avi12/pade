<script lang="ts">
  import { windowFacts } from "@/lib/dashboard/agent-board";
  import type { BoardWindow } from "@/lib/dashboard/agent-board";
  import Icon from "@/lib/Icon.svelte";
  import AgentRow from "@/panels/dashboard/AgentRow.svelte";
  import ChangedFiles from "@/panels/dashboard/ChangedFiles.svelte";

  // One project window on the dashboard: its name (a click brings the window
  // forward), how much it has changed, and each of its agents.
  const { board, now, onfocus }: {
    board: BoardWindow;
    now: number;
    onfocus: (sessionId?: string) => Promise<void>;
  } = $props();
</script>

<section class="card">
  <button class="title card-label" data-tooltip={board.project} onclick={() => onfocus()} type="button">
    <Icon name="folder" size={15} />
    <span class="project">{board.projectLabel}</span>
    <Icon name="external" size={13} />
  </button>

  <p class="meta">{windowFacts({
    board,
    now
  })}</p>

  <ul class="agents">
    {#each board.agents as agent (agent.id)}
      <AgentRow {agent} {now} onfocus={() => onfocus(agent.id)} />
    {/each}
  </ul>

  <ChangedFiles files={board.changedFiles} />
</section>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
  }

  .title {
    display: flex;
    gap: 8px;
    align-items: center;
    inline-size: 100%;
    border-block-start: none;
    border-inline: none;
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 700;
    font-size: 14px;
    text-align: start;
    cursor: pointer;

    &:hover,
    &:focus-visible {
      color: var(--primary);
      filter: none;
    }

    .project {
      flex: 1;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .meta {
    margin: 0;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 11px;
  }

  .agents {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
