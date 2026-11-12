<script lang="ts">
  // The starting model and effort per installed agent, for one scope: every
  // PADE workspace (`project` null) or this one, which outranks it. Only
  // agents whose own catalog PADE can read are listed - an agent you don't
  // have never shows up. The tab's own menu can still switch a session after.

  import {
    globalModelDefault,
    projectModelDefault,
    withGlobalModelDefault,
    withProjectModelDefault
  } from "@/lib/model/agent-model-scope";
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { prefs } from "@/lib/settings/settings.svelte";
  import { modelOption, modelOptions, refreshModelOptions } from "@/lib/stores/model/modelOptions.svelte";
  import type { Agent, ModelChoice } from "@/lib/types/agent";
  import AgentModelDefaultRow from "@/panels/config/models/AgentModelDefaultRow.svelte";
  import { onMount } from "svelte";

  const { agents, project }: {
    agents: Agent[];
    /** The workspace these picks are for, or null for every workspace. */
    project: string | null;
  } = $props();

  const AGENT_OWN_DEFAULT = "Its own default";

  // Names the scope in each pick's element id - both lists sit in one panel.
  const scope = $derived.by(() => {
    if (project === null) {
      return "global";
    }

    return "project";
  });
  const configurableAgents = $derived(agents.filter(agent => modelOptions(agent.command).length > 0));

  // Re-read on every visit: an agent re-fetches its catalog, and a list
  // loaded once would keep the models it had then.
  onMount(async () => {
    for (const agent of agents) {
      await refreshModelOptions(agent.command);
    }
  });

  function chosenFor(agent: Agent) {
    if (project === null) {
      return globalModelDefault({
        prefs,
        agent: agent.id
      });
    }

    return projectModelDefault({
      prefs,
      project,
      agent: agent.id
    });
  }

  /** What leaving the pick empty means here: the agent's own default, or - in
   *  a workspace - every workspace's pick when one is set. */
  function fallbackLabel(agent: Agent) {
    const inherited = project === null ? undefined : globalModelDefault({
      prefs,
      agent: agent.id
    });
    if (inherited?.model === undefined) {
      return AGENT_OWN_DEFAULT;
    }

    const name = modelOption({
      command: agent.command,
      model: inherited.model
    })?.name ?? inherited.model;
    return `Every workspace's (${name})`;
  }

  async function save({ agent, choice }: {
    agent: Agent;
    choice: ModelChoice | undefined;
  }) {
    if (project === null) {
      await updatePrefs({
        agentModelGlobal: withGlobalModelDefault({
          prefs,
          agent: agent.id,
          choice
        })
      });
      return;
    }

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

{#if configurableAgents.length === 0}
  <p class="field-hint">No installed agent shares a model list yet</p>
{:else}
  <ul class="agent-defaults">
    {#each configurableAgents as agent (agent.id)}
      <AgentModelDefaultRow
        {agent}
        chosen={chosenFor(agent)}
        fallbackLabel={fallbackLabel(agent)}
        onsave={async choice => await save({
          agent,
          choice
        })}
        {scope}
      />
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
</style>
