<script lang="ts">
  import { UI_SCALE_MAXIMUM, UI_SCALE_MINIMUM, UI_SCALE_STEP } from "@/lib/settings/prefs-bounds";
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import { steppedUiScale } from "@/lib/settings/ui-scale";
  import { formatPercent } from "@/lib/util/format";

  // The UI + terminal zoom, stepped A− / A+ within its persisted bounds.
  const scalePercent = $derived(formatPercent(effective.uiScale * 100));

  async function stepScale(delta: number) {
    await updatePrefs({
      uiScale: steppedUiScale({
        scale: effective.uiScale,
        delta
      })
    });
  }
</script>

<div class="field-row">
  <span class="field-text">
    <span class="field-label">Font size</span>
    <span class="field-hint">Applies to UI and terminal</span>
  </span>
  <div class="stepper">
    <button
      class="step"
      aria-label="Decrease font size"
      disabled={effective.uiScale <= UI_SCALE_MINIMUM}
      onclick={() => stepScale(-UI_SCALE_STEP)}
    >A−</button>
    <output class="scale-value">{scalePercent}</output>
    <button
      class="step step-up"
      aria-label="Increase font size"
      disabled={effective.uiScale >= UI_SCALE_MAXIMUM}
      onclick={() => stepScale(UI_SCALE_STEP)}
    >A+</button>
  </div>
</div>

<style>
  .stepper {
    display: inline-flex;
    flex: none;
    gap: 4px;
    align-items: center;
    padding: 4px;
    border-radius: 12px;
    background: var(--surface-2);

    .step {
      display: inline-flex;
      justify-content: center;
      align-items: center;
      block-size: 28px;
      inline-size: 28px;
      border: none;
      border-radius: 9px;
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-weight: 800;
      font-size: 13px;
      cursor: pointer;
      transition: background 140ms var(--ease);

      &:disabled {
        opacity: 40%;
        cursor: default;
      }

      &:hover:not(:disabled) {
        background: var(--surface-3);
      }
    }

    .step-up {
      font-size: 17px;
    }

    .scale-value {
      min-inline-size: 44px;
      font-weight: 700;
      font-size: 12px;
      text-align: center;
    }
  }
</style>
