<script lang="ts">
  import { globalNaming, projectNamingChoice, withProjectNamingChoice } from "@/lib/agent/ai-naming-scope";
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { prefs } from "@/lib/settings/settings.svelte";
  import Segmented from "@/lib/ui/controls/Segmented.svelte";

  // The project scope of "name session tabs with AI". Three states, because the
  // absence of a project choice is itself a state: it follows the picker's global
  // default, and picking `Global` is the only way back to it.
  const { project }: { project: string } = $props();

  const namingChoice = $derived(
    projectNamingChoice({
      prefs,
      project
    })
  );
  const globalNamingWord = $derived.by(() => {
    if (globalNaming(prefs)) {
      return "on";
    }

    return "off";
  });
  const namingScopes = [
    {
      value: undefined,
      label: "Global"
    },
    {
      value: true,
      label: "On"
    },
    {
      value: false,
      label: "Off"
    }
  ] as const satisfies readonly {
    value: boolean | undefined;
    label: string;
  }[];
</script>

<div class="naming-row">
  <span class="field-text">
    <span class="field-label">Name tabs with AI</span>
    <span class="field-hint">
      Each tab is renamed from what its agent is doing, refreshed while it works
    </span>
  </span>
  <div class="naming-scope">
    <Segmented
      chosen={namingChoice}
      label="Name tabs with AI, in this project"
      onchoose={async choice => await updatePrefs({
        aiSessionNamingProjects: withProjectNamingChoice({
          prefs,
          project,
          choice
        })
      })}
      options={namingScopes}
    />
  </div>
</div>
<p class="field-hint">
  {#if namingChoice === undefined}
    Following the global setting, which is {globalNamingWord}. Change it for every project in the
    project picker.
  {:else}
    Set for this project only - the global setting is {globalNamingWord}. Pick
    <strong>Global</strong> to follow it again.
  {/if}
</p>

<style>
  .naming-row {
    /* Label beside the control while the panel is wide; stacked once the drag
       handle takes that room away. Asked of the card, never the viewport. */
    @container (inline-size < 400px) {
      flex-direction: column;
      align-items: stretch;
    }

    display: flex;
    gap: 12px;
    justify-content: space-between;
    align-items: center;
  }

  /* Three equal scopes, always in one row - they are short words, and stacking
     them would read as three settings rather than one choice. */
  .naming-scope {
    flex: none;
    min-inline-size: 170px;
  }
</style>
