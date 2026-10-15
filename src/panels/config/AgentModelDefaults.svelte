<script lang="ts">
  // The workspace's starting model and effort, per installed agent. Only agents
  // whose own catalog PADE can read are listed - an agent you don't have never
  // shows up. A new session of that agent in this workspace starts on the pick;
  // the tab's own menu can still switch it afterwards.

  import { projectModelDefault, withProjectModelDefault } from "@/lib/model/agent-model-scope";
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { prefs } from "@/lib/settings/settings.svelte";
  import { ensureModelOptions, modelOptions } from "@/lib/stores/model/modelOptions.svelte";
  import type { Agent } from "@/lib/types";
  import AgentModelDefaultRow from "@/panels/config/AgentModelDefaultRow.svelte";
  import { onMount } from "svelte";

  const { agents, project }: {
    agents: Agent[];
    project: string;
  } = $props();

  const configurableAgents = $derived(agents.filter(agent => modelOptions(agent.command).length > 0));

  onMount(async () => {
    for (const agent of agents) {
      await ensureModelOptions(agent.command);
    }
  });
</script>

{#if configurableAgents.length === 0}
  <p class="field-hint">No installed agent shares a model list yet</p>
{:else}
  <ul class="agent-defaults">
    {#each configurableAgents as agent (agent.id)}
      <AgentModelDefaultRow
        {agent}
        chosen={projectModelDefault({
          prefs,
          project,
          agent: agent.id
        })}
        onsave={async choice => await updatePrefs({
          agentModelProjects: withProjectModelDefault({
            prefs,
            project,
            agent: agent.id,
            choice
          })
        })}
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
