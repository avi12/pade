<script lang="ts">
  // The model + effort menu a tab's model tag opens. Models come from the
  // agent's own catalog; the effort row climbs the shown model's own ladder
  // with the same rising bars the tab shows, so menu and tag read alike. A pick
  // goes to `onchoose`, which applies it the moment the agent can take it (see
  // stores/modelSwitch).
  import EffortPicker from "@/lib/EffortPicker.svelte";
  import { modelLabel, modelName } from "@/lib/model-label";
  import ModelPicker from "@/lib/ModelPicker.svelte";
  import { sessionModel } from "@/lib/stores/context.svelte";
  import { modelOption, modelOptions } from "@/lib/stores/modelOptions.svelte";
  import { pendingModelSwitch, SwitchWait } from "@/lib/stores/modelSwitch.svelte";
  import type { AgentSession, ModelChoice } from "@/lib/types";

  const { session, onchoose }: {
    session: AgentSession;
    onchoose: (choice: ModelChoice) => Promise<void>;
  } = $props();

  // What a pending switch is waiting for, in the user's words.
  const WAIT_NOTES: Record<SwitchWait, string> = {
    [SwitchWait.applying]: "applying…",
    [SwitchWait.quiet]: "resumes on it once the agent is idle",
    [SwitchWait.successor]: "the session taking over from the handoff starts on it"
  };

  const running = $derived(sessionModel(session.id));
  const pending = $derived(pendingModelSwitch(session.id));
  const options = $derived(modelOptions(session.agent.command));
  // What the menu shows as current: a pick not yet applied, else what runs.
  const shownModel = $derived(pending?.choice.model ?? running?.id ?? null);
  const shownEffort = $derived(pending?.choice.effort ?? running?.effort ?? null);
  const shownOption = $derived.by(() => {
    if (shownModel === null) {
      return undefined;
    }

    return modelOption({
      command: session.agent.command,
      model: shownModel
    });
  });
  const currentModelId = $derived(shownOption?.id ?? null);
  const effortLadder = $derived(shownOption?.efforts ?? []);
  const recommendedEffort = $derived(shownOption?.recommendedEffort ?? null);
  const pendingNote = $derived.by(() => {
    if (pending === null) {
      return null;
    }

    const shown = modelLabel({
      name: modelName(shownModel ?? ""),
      effort: shownEffort
    });
    return `${shown} - ${WAIT_NOTES[pending.waitingFor]}`;
  });
</script>

<section class="model-menu-body">
  <header class="menu-separator">{session.agent.label} · model</header>
  {#if options.length === 0}
    <p class="note">No model list from {session.agent.label} yet</p>
  {:else}
    <ModelPicker current={currentModelId} onpick={async model => await onchoose({ model })} {options} />
  {/if}

  {#if effortLadder.length > 0}
    <header class="menu-separator">Effort</header>
    <EffortPicker
      current={shownEffort}
      efforts={effortLadder}
      onpick={async effort => await onchoose({ effort })}
      recommended={recommendedEffort}
    />
  {/if}

  {#if pendingNote !== null}
    <p class="note pending">{pendingNote}</p>
  {/if}
</section>

<style>
  .model-menu-body {
    display: grid;
    gap: 4px;
    min-inline-size: 240px;
  }

  .note {
    margin: 0;
    padding: 6px 10px;
    color: var(--on-surface-variant);
    font-size: 12px;

    &.pending {
      color: var(--primary);
    }
  }
</style>
