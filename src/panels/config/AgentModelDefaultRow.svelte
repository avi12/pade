<script lang="ts">
  // One agent's starting model for the workspace: a pick from its own catalog,
  // then the picked model's effort ladder.

  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import EffortPicker from "@/lib/model/EffortPicker.svelte";
  import { modelOption, modelOptions } from "@/lib/stores/model/modelOptions.svelte";
  import type { Agent, ModelChoice } from "@/lib/types";

  const { agent, chosen, onsave }: {
    agent: Agent;
    /** The workspace's current pick for this agent, if any. */
    chosen: ModelChoice | undefined;
    onsave: (choice: ModelChoice | undefined) => Promise<void>;
  } = $props();

  const AGENT_DEFAULT = "";

  const options = $derived(modelOptions(agent.command));
  const selectedModel = $derived(chosen?.model ?? AGENT_DEFAULT);
  const chosenOption = $derived(
    modelOption({
      command: agent.command,
      model: selectedModel
    })
  );
  const efforts = $derived(chosenOption?.efforts ?? []);
  const recommendedEffort = $derived(chosenOption?.recommendedEffort ?? null);
  const selectedEffort = $derived(chosen?.effort ?? null);
  const selectId = $derived(`model-default-${agent.id}`);
</script>

<li class="agent-default">
  <label class="agent-model" for={selectId}>
    <span class="agent-name"><Icon name={agentIconName(agent.id)} size={14} />{agent.label}</span>
    <select
      id={selectId}
      onchange={async e => {
        const model = e.currentTarget.value;
        const isAgentDefault = model === AGENT_DEFAULT;
        await onsave(isAgentDefault ? undefined : { model });
      }}
      value={selectedModel}
    >
      <option value={AGENT_DEFAULT}>Its own default</option>
      {#each options as option (option.id)}
        <option value={option.id}>{option.name}</option>
      {/each}
    </select>
  </label>
  {#if efforts.length > 0}
    <EffortPicker
      current={selectedEffort}
      {efforts}
      onpick={async effort => await onsave({
        ...chosen,
        effort
      })}
      recommended={recommendedEffort}
    />
  {/if}
</li>

<style>
  .agent-default {
    display: grid;
    gap: 4px;
  }

  .agent-model {
    display: flex;
    gap: 10px;
    justify-content: space-between;
    align-items: center;

    .agent-name {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      font-weight: 600;
      font-size: 13px;
    }

    select {
      max-inline-size: 60%;
      font: inherit;
      font-size: 12px;
    }
  }
</style>
