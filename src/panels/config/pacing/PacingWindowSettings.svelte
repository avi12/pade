<script lang="ts">
  import type { PacedWindowKind, PacingSettings } from "@/lib/types";
  import Segmented from "@/lib/ui/controls/Segmented.svelte";
  import {
    PACED_WINDOW_TEXT,
    PACING_PRESETS,
    PacingPreset,
    pacingPreset,
    withPreset
  } from "@/lib/usage/pacing-policy";
  import PacingFineTune from "@/panels/config/pacing/PacingFineTune.svelte";
  import PacingForecast from "@/panels/config/pacing/PacingForecast.svelte";
  import PacingWindowHeader from "@/panels/config/pacing/PacingWindowHeader.svelte";

  // One paced window's settings (lib/usage/pacing-policy): its strength preset
  // up front, its live line, and the margin and steps folded under Fine-tune.
  // In the workspace scope it says whether it follows the global settings, and
  // offers the way back once it doesn't.
  const { agent, kind, settings, onsave, inherit }: {
    /** The active agent, whose account the live line reads. */
    agent: string;
    kind: PacedWindowKind;
    settings: PacingSettings;
    onsave: (next: PacingSettings) => Promise<void>;
    /** In the workspace scope: whether this window follows the global settings,
     *  and how it goes back to them. Absent in the global scope. */
    inherit?: {
      isFollowing: boolean;
      onreset: () => Promise<void>;
    };
  } = $props();

  const text = $derived(PACED_WINDOW_TEXT[kind]);
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
</script>

<section class="window">
  <PacingWindowHeader {inherit} {kind} />

  <div class="field">
    <Segmented
      chosen={preset}
      label={`${text.title} pacing strength`}
      onchoose={async value => await onsave(
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
    <PacingForecast {agent} {kind} {settings} />
    <PacingFineTune {kind} {onsave} {settings} />
  {/if}
</section>

<style>
  .window {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-inline-start: 10px;
    border-inline-start: 2px solid var(--outline);
  }
</style>
