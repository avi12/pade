<script lang="ts">
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import { PacingAction, PacingMode } from "@/lib/types";
  import Checkbox from "@/lib/ui/Checkbox.svelte";
  import Segmented from "@/lib/ui/Segmented.svelte";

  // Usage pacing's settings (lib/stores/usage/usagePacing): what happens to an
  // agent spending its usage too fast - or nothing, the pacing mode's `off` -
  // whether its effort and model step down first, and what a stopped agent does
  // once it has room again.
  const actionOptions = [
    {
      value: PacingAction.enum.stop,
      label: "Stop",
      icon: "close"
    },
    {
      value: PacingAction.enum.slow,
      label: "Slow down",
      icon: "clock"
    },
    {
      value: PacingMode.enum.off,
      label: "Off",
      icon: "minimize"
    }
  ] as const;

  const resumeOptions = [
    {
      value: PacingMode.enum.ask,
      label: "Ask me",
      icon: "more"
    },
    {
      value: PacingMode.enum.continue,
      label: "Continue",
      icon: "refresh"
    }
  ] as const;

  const isOff = $derived(effective.pacingMode === PacingMode.enum.off);
  const chosenAction = $derived.by(() => {
    if (isOff) {
      return PacingMode.enum.off;
    }

    return effective.pacingAction;
  });

  async function chooseAction(value: PacingAction | typeof PacingMode.enum.off) {
    if (value === PacingMode.enum.off) {
      await updatePrefs({ usagePacing: value });
      return;
    }

    await updatePrefs({
      pacingAction: value,
      usagePacing: isOff ? PacingMode.enum.ask : effective.pacingMode
    });
  }
</script>

<div class="field">
  <span class="field-text">
    <span class="field-label">Usage pacing</span>
    <span class="field-hint">What happens to an agent that would use up its usage long before the reset</span>
  </span>
  <Segmented
    chosen={chosenAction}
    label="Usage pacing"
    onchoose={async value => await chooseAction(value)}
    options={actionOptions}
  />
</div>

{#if !isOff}
  <label class="check">
    <Checkbox checked={effective.pacingAdapt} onchange={checked => updatePrefs({ pacingAdapt: checked })} />
    <span class="field-text">
      <span class="field-label">Turn effort and model down first</span>
      <span class="field-hint">Steps back up when there's room, and back to your choice at the reset</span>
    </span>
  </label>
{/if}

{#if chosenAction === PacingAction.enum.stop}
  <div class="field">
    <span class="field-text">
      <span class="field-label">Once a stopped agent has room</span>
      <span class="field-hint">It always continues on its own just before the reset</span>
    </span>
    <Segmented
      chosen={effective.pacingMode}
      label="Once a stopped agent has room"
      onchoose={value => updatePrefs({ usagePacing: value })}
      options={resumeOptions}
    />
  </div>
{/if}

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .field-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    line-height: 1.3;
  }

  .field-label {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 12px;
  }

  .field-hint {
    color: var(--on-surface-variant);
    font-size: 10px;
  }

  .check {
    display: flex;
    gap: 10px;
    align-items: center;
    min-inline-size: 0;
    cursor: pointer;
  }
</style>
