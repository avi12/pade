<script lang="ts">
  import { agentIconName } from "@/lib/agent-icon";
  import { agents as agentsApi } from "@/lib/bridge";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import { SHELL_AGENT_ID } from "@/lib/types";
  import type { Agent } from "@/lib/types";

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
    <div class="chips" aria-label="Default agent" role="radiogroup">
      {#each realAgents as agent (agent.id)}
        {@const isSelected = defaultAgent === agent.id}
        <button
          class="chip"
          class:on={isSelected}
          aria-checked={isSelected}
          data-agent={agent.id}
          onclick={() => onpick(agent.id)}
          role="radio"
        >
          <Icon name={agentIconName(agent.id)} size={15} />{agent.label}
        </button>
      {/each}
    </div>
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

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  /* Choice chips — pills; selected gets a primary edge over its container, with
     the agent's own glyph leading the label. The glyph carries the agent's brand
     colour (--agent-brand, set per data-agent in theme.css); a monochrome-brand
     agent has none and its glyph follows the label colour. */
  .chip {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding: 8px 16px;
    border: 1px solid transparent;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: border-color 150ms var(--ease);

    :global(.icon) {
      color: var(--agent-brand, currentColor);
    }

    &:hover {
      border-color: var(--primary);
    }

    &.on {
      border-color: var(--primary);
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }
</style>
