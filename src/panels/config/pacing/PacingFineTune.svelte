<script lang="ts">
  import type { PacedWindowKind, PacingSettings } from "@/lib/types";
  import Select from "@/lib/ui/controls/Select.svelte";
  import { marginLabel, PACED_WINDOW_TEXT, PACING_EARLY_OPTIONS } from "@/lib/usage/pacing-policy";
  import PacingSteps from "@/panels/config/pacing/PacingSteps.svelte";

  // A paced window's Fine-tune fold: how far ahead of pace it may run before
  // pacing acts (in the window's own scale - minutes for a session, hours and
  // days for a week), and the steps pacing may take.
  const { kind, settings, onsave }: {
    kind: PacedWindowKind;
    settings: PacingSettings;
    onsave: (next: PacingSettings) => Promise<void>;
  } = $props();

  const text = $derived(PACED_WINDOW_TEXT[kind]);
  const earlyOptions = $derived(
    PACING_EARLY_OPTIONS[kind].map(minutes => ({
      value: String(minutes),
      label: marginLabel(minutes)
    }))
  );
  const earlySelectId = $derived(`pacing-early-${kind}`);
</script>

<details class="tune">
  <summary class="field-label">Fine-tune</summary>
  <label class="trigger" for={earlySelectId}>
    <span class="field-label">{text.trigger}</span>
    <Select
      id={earlySelectId}
      onchange={async value => await onsave({
        ...settings,
        earlyMinutes: Number(value)
      })}
      options={earlyOptions}
      value={String(settings.earlyMinutes)}
    />
    {#if text.triggerEnd}
      <span class="field-label">{text.triggerEnd}</span>
    {/if}
  </label>
  <div class="field">
    <span class="field-label">Steps, one at a time while it stays too fast</span>
    <PacingSteps {onsave} {settings} />
  </div>
</details>

<style>
  .tune summary {
    cursor: pointer;
  }

  .tune[open] summary {
    margin-block-end: 10px;
  }

  .trigger {
    --select-width: 90px;

    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    margin-block-end: 10px;
  }
</style>
