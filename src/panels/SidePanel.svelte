<script lang="ts">
  import { Side, SIDE_PANEL_TABS } from "@/lib/shell/side-panels";
  import { startRunner } from "@/lib/stores/tasks/runners.svelte";
  import type { Agent } from "@/lib/types";
  import LazyPanel from "@/lib/ui/LazyPanel.svelte";
  import ChangeFeed from "@/panels/ChangeFeed.svelte";
  import SidePanelHeader from "@/panels/SidePanelHeader.svelte";

  // The open side panel under its shared header. The Change Feed is the default
  // and loads with the app; the others load the first time they open.
  const { side, project, agent, agents }: {
    side: Side;
    project: string;
    /** The active session's agent id, so Config shows its files. */
    agent: string;
    /** Installed agents, excluding the shell fallback. */
    agents: Agent[];
  } = $props();

  const title = $derived(SIDE_PANEL_TABS.find(tab => tab.id === side)?.label ?? "");
</script>

<aside class="side-pane">
  <SidePanelHeader {title} />

  <div class="panel-body">
    {#if side === Side.Feed}
      <ChangeFeed {project} />
    {:else if side === Side.Vcs}
      <LazyPanel load={() => import("@/panels/VcsPanel.svelte")}>
        {#snippet children(VcsPanel)}
          <VcsPanel {project} />
        {/snippet}
      </LazyPanel>
    {:else if side === Side.Tasks}
      <LazyPanel load={() => import("@/panels/TasksPanel.svelte")}>
        {#snippet children(TasksPanel)}
          <!-- Run a project task as a streaming runner in the dock (not a
               throwaway terminal tab), so its output stays visible and can be
               piped into an agent. -->
          <TasksPanel
            onrun={async task => await startRunner({
              ...task,
              project
            })} {project} />
        {/snippet}
      </LazyPanel>
    {:else}
      <LazyPanel load={() => import("@/panels/ConfigPanel.svelte")}>
        {#snippet children(ConfigPanel)}
          <ConfigPanel {agent} {agents} {project} />
        {/snippet}
      </LazyPanel>
    {/if}
  </div>
</aside>

<style>
  .side-pane {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-block-size: 0;
    min-inline-size: 0;
    border-inline-start: 1px solid var(--outline);
    background: var(--surface);
    animation: panel-in 340ms var(--ease);

    /* Cyberpunk: the seam the panel hangs off is one of the HUD's brightest
       lines, with the top bar and the panel header, and blooms like them. */
    :global(:root[data-palette="cyberpunk"]) & {
      border-inline-start: 2px solid var(--hairline);
      box-shadow: var(--hairline-glow);
    }
  }

  .panel-body {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-block-size: 0;
  }
</style>
