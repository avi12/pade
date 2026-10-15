<script lang="ts">
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import type { PacingSettings } from "@/lib/types";
  import Segmented from "@/lib/ui/Segmented.svelte";
  import Select from "@/lib/ui/Select.svelte";
  import {
    PACING_EARLY_OPTIONS,
    PACING_PRESETS,
    PacingPreset,
    pacingPreset,
    withPreset
  } from "@/lib/usage/pacing-policy";
  import PacingForecast from "@/panels/config/PacingForecast.svelte";
  import PacingSteps from "@/panels/config/PacingSteps.svelte";

  // Usage pacing's settings (lib/usage/pacing-policy): a strength preset that
  // sets every step at once, when to act, a live line for the active agent, and
  // the steps themselves. Editing a step turns the preset into Custom.
  const { agent }: {
    /** The active agent, whose account the live line reads. */
    agent: string;
  } = $props();

  const MINUTES_PER_HOUR = 60;
  const EARLY_SELECT_ID = "pacing-early";

  const settings = $derived(effective.pacing);
  const preset = $derived(pacingPreset(settings));
  const isOn = $derived(preset !== PacingPreset.Off);
  const presetOptions = $derived.by(() => {
    if (preset === PacingPreset.Custom) {
      return [...PACING_PRESETS, {
        value: PacingPreset.Custom,
        label: "Custom",
        hint: "Your own set of steps"
      }];
    }

    return [...PACING_PRESETS];
  });
  const presetHint = $derived(presetOptions.find(option => option.value === preset)?.hint ?? "");
  const earlyOptions = PACING_EARLY_OPTIONS.map(minutes => ({
    value: String(minutes),
    label: minutes < MINUTES_PER_HOUR ? `${minutes} min` : `${minutes / MINUTES_PER_HOUR} h`
  }));

  async function save(next: PacingSettings) {
    await updatePrefs({ pacing: next });
  }
</script>

<div class="field">
  <span class="field-label">Strength</span>
  <Segmented
    chosen={preset}
    label="Usage pacing strength"
    onchoose={async value => await save(
      withPreset({
        settings,
        preset: value
      })
    )}
    options={presetOptions}
  />
  <span class="field-hint">{presetHint}</span>
</div>

{#if isOn}
  <label class="trigger" for={EARLY_SELECT_ID}>
    <span class="field-label">Act when on track to run out</span>
    <Select
      id={EARLY_SELECT_ID}
      onchange={async value => await save({
        ...settings,
        earlyMinutes: Number(value)
      })}
      options={earlyOptions}
      value={String(settings.earlyMinutes)}
    />
    <span class="field-label">early</span>
  </label>

  <PacingForecast {agent} {settings} />

  <div class="field">
    <span class="field-label">Steps, one at a time while it stays too fast</span>
    <PacingSteps onsave={save} {settings} />
  </div>

  <p class="field-hint">Every step undoes itself as room comes back, and everything reverts when the window resets</p>
{/if}

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .field-label {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 12px;
  }

  .field-hint {
    margin: 0;
    color: var(--on-surface-variant);
    font-size: 10px;
  }

  .trigger {
    --select-width: 90px;

    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
</style>
