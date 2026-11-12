<script lang="ts">
  // One agent's starting model for the workspace: a pick from its own catalog,
  // then the picked model's effort ladder.

  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import EffortPicker from "@/lib/model/menu/EffortPicker.svelte";
  import { modelOption, modelOptions, refreshModelOptions } from "@/lib/stores/model/modelOptions.svelte";
  import type { Agent, ModelChoice } from "@/lib/types";
  import Select from "@/lib/ui/controls/Select.svelte";

  const { agent, chosen, fallbackLabel, scope, onsave }: {
    agent: Agent;
    /** This scope's current pick for this agent, if any. */
    chosen: ModelChoice | undefined;
    /** What no pick here falls back to, as the first option reads it. */
    fallbackLabel: string;
    /** Which list the row is in - its pick's id must differ between them. */
    scope: string;
    onsave: (choice: ModelChoice | undefined) => Promise<void>;
  } = $props();

  const AGENT_DEFAULT = "";

  const modelChoices = $derived([
    {
      value: AGENT_DEFAULT,
      label: fallbackLabel
    },
    ...modelOptions(agent.command).map(option => ({
      value: option.id,
      label: option.name
    }))
  ]);
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
  const selectId = $derived(`model-default-${scope}-${agent.id}`);
</script>

<li class="agent-default">
  <!-- Focusing the pick re-reads the agent's catalog, as the tab's model menu
       does on opening, so a model the agent fetched since is listed. -->
  <label class="agent-model" for={selectId} onfocusin={async () => await refreshModelOptions(agent.command)}>
    <span class="agent-name"><Icon name={agentIconName(agent.id)} size={14} />{agent.label}</span>
    <Select
      id={selectId}
      onchange={async model => {
        const isAgentDefault = model === AGENT_DEFAULT;
        await onsave(isAgentDefault ? undefined : { model });
      }}
      options={modelChoices}
      value={selectedModel}
    />
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
      font-size: 0.8125rem;
    }
  }
</style>
