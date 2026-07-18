<script lang="ts">
  import { agents as agentsApi } from "@/lib/bridge";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import { SHELL_AGENT_ID } from "@/lib/types";
  import type { Agent } from "@/lib/types";
  import AgentChips from "@/panels/picker/AgentChips.svelte";

  // Default-agent section: owns its own agent list so it can re-detect
  // (Reload) without needing a new prop from the parent. It's seeded from —
  // and kept in sync with — the `agents` prop; a rescan flag drives the
  // spinning refresh icon + skeleton chips. The default-agent choice persists
  // with the parent (single settings owner) via `onpick`.
  const { agents, defaultAgent, onpick }: {
    agents: Agent[];
    defaultAgent: string | null;
    onpick: (agentId: string) => void;
  } = $props();

  let agentList = $state<Agent[]>([]);
  let scanning = $state(false);
  // The exact `agents` prop reference we last adopted. A local rescan writes a
  // fresh array into `agentList`; without this guard the effect would re-run
  // whenever `scanning` (or any other read state) changes and clobber that
  // result with the stale prop. Plain (non-reactive) bookkeeping so it neither
  // registers as a dependency nor retriggers the effect. We adopt the prop only
  // when its reference itself changes.
  let lastAdopted: Agent[] | null = null;
  $effect(() => {
    const incoming = agents;
    if (incoming === lastAdopted) {
      return;
    }

    lastAdopted = incoming;
    agentList = incoming;
  });

  const realAgents = $derived(agentList.filter(agent => agent.id !== SHELL_AGENT_ID));
  const showSkeleton = $derived(scanning && realAgents.length === 0);
  const showEmpty = $derived(!scanning && realAgents.length === 0);
  const agentStatus = $derived(agentStatusText());

  function agentStatusText(): string {
    if (scanning) {
      return "Scanning installs…";
    }

    if (realAgents.length === 0) {
      return "No agents found";
    }

    return `${formatCount(realAgents.length)} detected on this machine`;
  }
</script>

<section class="master">
  <div class="master-head">
    <div class="master-title">
      <h2>Default agent</h2>
      <output class="agent-status">{agentStatus}</output>
    </div>
    <button
      class="rescan"
      class:scanning
      aria-label="Rescan for installed agents"
      data-tooltip="Rescan for installed agents"
      onclick={async () => {
        scanning = true;
        try {
          agentList = await agentsApi.detect();
        } finally {
          scanning = false;
        }
      }}
    ><Icon name="refresh" size={14} /> Reload</button>
  </div>

  {#if showSkeleton}
    <div class="agent-skels" aria-hidden="true">
      <span class="agent-skel"></span>
      <span style:animation-delay="0.15s" class="agent-skel"></span>
      <span style:animation-delay="0.3s" class="agent-skel"></span>
    </div>
  {:else if showEmpty}
    <p class="agent-empty">
      No supported agents were found on this machine. Install one (Claude
      Code, Codex, Gemini CLI…) then press <strong>Reload</strong>.
    </p>
  {:else}
    <AgentChips agents={realAgents} ariaLabel="Default agent" {onpick} selected={defaultAgent} />
  {/if}
</section>

<style>
  .master-head {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    justify-content: space-between;
    align-items: center;
  }

  .master-title {
    display: flex;
    gap: 10px;
    align-items: baseline;
    min-inline-size: 0;
  }

  .agent-status {
    color: var(--on-surface-variant);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  /* The Reload pill (.rescan) is shared chrome — see picker/chrome.css. */

  /* Skeleton chips while the first scan runs. */
  .agent-skels {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .agent-skel {
    block-size: 35px;
    border-radius: 999px;
    background: var(--surface-2);
    animation: pulse 1100ms var(--ease) infinite;

    &:nth-child(1) {
      inline-size: 112px;
    }

    &:nth-child(2) {
      inline-size: 86px;
    }

    &:nth-child(3) {
      inline-size: 98px;
    }
  }

  .agent-empty {
    margin: 0;
    padding: 14px 16px;
    border: 1px dashed var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-size: 13px;

    strong {
      color: var(--on-surface);
    }
  }

  /* The agent choice-chips live in picker/AgentChips.svelte (shared with the
     create form's Agent row). */
</style>
