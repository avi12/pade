<script lang="ts">
  // The workspace's starting model and effort, per installed agent. Only agents
  // whose own catalog PADE can read are listed - an agent you don't have never
  // shows up. A new session of that agent in this workspace starts on the pick;
  // the tab's own menu can still switch it afterwards.

  import { agentIconName } from "@/lib/agent-icon";
  import { projectModelDefault, withProjectModelDefault } from "@/lib/agent-model-scope";
  import EffortPicker from "@/lib/EffortPicker.svelte";
  import Icon from "@/lib/Icon.svelte";
  import { updatePrefs } from "@/lib/prefs.svelte";
  import { prefs } from "@/lib/settings.svelte";
  import { ensureModelOptions, modelOption, modelOptions } from "@/lib/stores/modelOptions.svelte";
  import type { Agent, ModelChoice } from "@/lib/types";
  import { onMount } from "svelte";

  const { agents, project }: {
    agents: Agent[];
    project: string;
  } = $props();

  const AGENT_DEFAULT = "";

  const configurable = $derived(agents.filter(agent => modelOptions(agent.command).length > 0));

  onMount(async () => {
    for (const agent of agents) {
      await ensureModelOptions(agent.command);
    }
  });

  function currentDefault(agent: Agent) {
    return projectModelDefault({
      prefs,
      project,
      agent: agent.id
    });
  }

  function effortLadder(agent: Agent) {
    const model = currentDefault(agent)?.model;
    if (model === undefined) {
      return [];
    }

    return modelOption({
      command: agent.command,
      model
    })?.efforts ?? [];
  }

  async function saveDefault({ agent, choice }: {
    agent: Agent;
    choice: ModelChoice | undefined;
  }) {
    await updatePrefs({
      agentModelProjects: withProjectModelDefault({
        prefs,
        project,
        agent: agent.id,
        choice
      })
    });
  }
</script>

{#if configurable.length === 0}
  <p class="field-hint">No installed agent shares a model list yet</p>
{:else}
  <ul class="agent-defaults">
    {#each configurable as agent (agent.id)}
      {@const chosen = currentDefault(agent)}
      <li class="agent-default">
        <label class="agent-model" for={`model-default-${agent.id}`}>
          <span class="agent-name"><Icon name={agentIconName(agent.id)} size={14} />{agent.label}</span>
          <select
            id={`model-default-${agent.id}`}
            onchange={async e => {
              const model = e.currentTarget.value;
              const isAgentDefault = model === AGENT_DEFAULT;
              await saveDefault({
                agent,
                choice: isAgentDefault ? undefined : { model }
              });
            }}
            value={chosen?.model ?? AGENT_DEFAULT}
          >
            <option value={AGENT_DEFAULT}>Its own default</option>
            {#each modelOptions(agent.command) as option (option.id)}
              <option value={option.id}>{option.name}</option>
            {/each}
          </select>
        </label>
        {#if effortLadder(agent).length > 0}
          <EffortPicker
            current={chosen?.effort ?? null}
            efforts={effortLadder(agent)}
            onpick={async effort => await saveDefault({
              agent,
              choice: {
                ...chosen,
                effort
              }
            })}
            recommended={modelOption({
              command: agent.command,
              model: chosen?.model ?? ""
            })?.recommendedEffort ?? null}
          />
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .agent-defaults {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

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
