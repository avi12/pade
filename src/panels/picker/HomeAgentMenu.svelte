<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import type { AccountUsage, Agent } from "@/lib/types";
  import UsageLimitStack from "@/lib/usage/meter/UsageLimitStack.svelte";
  import { buildAgentGroup } from "@/lib/usage/usage-groups";
  import { settledValue } from "@/lib/util/settled";

  // The composer's LLM menu: every installed coding agent, its usage limits
  // beside it (the top bar's meter, for agents not running yet) so the pick
  // can follow what is left, and a tick on the picked one. Usage is read as
  // the menu opens - the backend caches each account for a few minutes.
  const { menuId, agents, selectedAgent, onpick }: {
    menuId: string;
    agents: readonly Agent[];
    selectedAgent: string | null;
    onpick: (agentId: string) => void;
  } = $props();

  let accounts = $state<ReadonlyMap<string, AccountUsage | null>>(new Map());

  async function loadAccounts() {
    const entries = await Promise.all(
      agents.map(async agent => [
        agent.id,
        await settledValue({
          promise: sendMessage(Command.usageAccountAgent, {
            agent: agent.id,
            forceRefresh: false
          }),
          fallback: null
        })
      ] as const)
    );
    accounts = new Map(entries);
  }

  const groups = $derived(
    agents.map(agent => buildAgentGroup({
      agent,
      account: accounts.get(agent.id) ?? null,
      now: Date.now()
    }))
  );
</script>

<ul
  id={menuId}
  style:position-anchor="--{menuId}"
  class="menu popover-menu"
  ontoggle={async e => {
    if (e.newState === "open") {
      await loadAccounts();
    }
  }}
  popover
>
  {#each groups as group (group.id)}
    {@const isPicked = selectedAgent === group.id}
    <li>
      <button
        class="menu-item"
        class:picked={isPicked}
        aria-current={isPicked}
        onclick={() => onpick(group.id)}
        popovertarget={menuId}
        popovertargetaction="hide"
        type="button"
      >
        <Icon name={group.icon} size={15} />
        <span class="menu-item-text">{group.name}</span>
        <span class="usage"><UsageLimitStack {group} /></span>
        <span class="tick" aria-hidden="true" hidden={!isPicked}>✓</span>
      </button>
    </li>
  {/each}
</ul>

<style>
  .usage {
    display: inline-flex;
    flex: none;
    margin-inline-start: auto;
    padding-inline-start: 12px;
  }

  /* The tick keeps its slot, so every row's usage lines up. */
  .tick[hidden] {
    display: inline;
    visibility: hidden;
  }
</style>
