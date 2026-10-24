<script lang="ts">
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import { MAXIMUM_HANDOFF_PERCENTAGE, MINIMUM_HANDOFF_PERCENTAGE } from "@/lib/usage/context-level";
  import { HandoffPercent, parseInput } from "@/lib/validate";

  // The context fill that hands a session off to a fresh agent: a slider and a
  // number box over the one pref.

  // Slider and number box both funnel through the input schema - the slider
  // can only emit in-band values, but the free-typed number is a trust
  // boundary like any other field.
  async function applyHandoffPercentage(raw: string) {
    const percent = parseInput({
      schema: HandoffPercent,
      raw
    });
    if (percent === null) {
      return;
    }

    await updatePrefs({ handoffPct: percent });
  }
</script>

<div class="field-row">
  <span class="field-text">
    <span class="field-label">Auto-handoff at</span>
    <span class="field-hint">Context fill that cycles the agent to a fresh one</span>
  </span>
  <div class="handoff-control">
    <input
      class="handoff-slider"
      aria-label="Auto-handoff threshold"
      max={MAXIMUM_HANDOFF_PERCENTAGE}
      min={MINIMUM_HANDOFF_PERCENTAGE}
      oninput={e => applyHandoffPercentage(e.currentTarget.value)}
      type="range"
      value={effective.handoffPercentage}
    />
    <span class="handoff-value">
      <input
        class="handoff-number"
        aria-label="Auto-handoff threshold percent"
        max={MAXIMUM_HANDOFF_PERCENTAGE}
        min={MINIMUM_HANDOFF_PERCENTAGE}
        onchange={e => {
          applyHandoffPercentage(e.currentTarget.value);
          // A rejected entry (out of band, not a number) snaps the box
          // back to the persisted value instead of lying at rest.
          e.currentTarget.value = String(effective.handoffPercentage);
        }}
        type="number"
        value={effective.handoffPercentage}
      />
    </span>
  </div>
</div>

<style>
  .handoff-control {
    display: inline-flex;
    flex: none;
    gap: 10px;
    align-items: center;

    .handoff-slider {
      inline-size: 110px;
      accent-color: var(--primary);
    }

    /* The percent sign lives visually inside the box, after the digits. */
    .handoff-value {
      position: relative;
      display: inline-flex;
      align-items: center;

      &::after {
        content: "%";
        position: absolute;
        inset-inline-end: 8px;
        color: var(--on-surface-variant);
        font-weight: 700;
        font-size: 0.75rem;
        pointer-events: none;
      }
    }

    .handoff-number {
      appearance: textfield;

      /* Two digits plus the inset % suffix; ch tracks the real digit width so
         a zoomed UI can never clip "30" down to a lone digit. */
      inline-size: calc(3ch + 2.5rem);
      padding-block: 6px;
      padding-inline: 8px 1.375rem;
      border: none;
      border-radius: 12px;
      background: var(--surface-2);
      color: var(--on-surface);
      font: inherit;
      font-weight: 700;
      font-size: 0.75rem;
      text-align: end;
    }
  }
</style>
