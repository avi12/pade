<script lang="ts">
  import type { IconName } from "@/lib/Icon.svelte";
  import { chooseThemeMode, effective } from "@/lib/settings/prefs.svelte";
  import { ThemeMode } from "@/lib/types";
  import Segmented from "@/lib/ui/Segmented.svelte";
  import FontSizeStepper from "@/panels/config/FontSizeStepper.svelte";
  import HandoffThreshold from "@/panels/config/HandoffThreshold.svelte";
  import TerminalColours from "@/panels/config/TerminalColours.svelte";
  import TerminalFontPicker from "@/panels/config/TerminalFontPicker.svelte";

  // The Appearance card's fields: theme mode, terminal font and colours, UI zoom
  // and the handoff threshold - each bound to a persisted pref via updatePrefs
  // (merge → apply → save through the shared store).
  const themeOptions = [
    {
      value: ThemeMode.enum.light,
      label: "Light",
      icon: "sun"
    },
    {
      value: ThemeMode.enum.dark,
      label: "Dark",
      icon: "moon"
    },
    {
      value: ThemeMode.enum.system,
      label: "Auto",
      icon: "monitor"
    },
    {
      value: ThemeMode.enum.cyberpunk,
      label: "Cyberpunk",
      icon: "bolt"
    }
  ] as const satisfies readonly {
    value: ThemeMode;
    label: string;
    icon: IconName;
  }[];
</script>

<div class="field">
  <span class="field-label">Theme</span>
  <div class="theme-choices">
    <Segmented
      chosen={effective.themeMode}
      label="Theme"
      onchoose={chooseThemeMode}
      options={themeOptions}
    />
  </div>
</div>

<TerminalFontPicker />

<TerminalColours />

<FontSizeStepper />

<HandoffThreshold />

<style>
  /* Four themes: a 2×2 block, and one row once there is room for four labels.
     Both sides are container queries, mutually exclusive, because a plain
     fallback declaration would sit AFTER the nested at-rule and win over it.
     Asked of the card, not the viewport - the side panel is drag-resizable, so a
     media query would answer about the wrong box. */
  .theme-choices {
    @container (inline-size < 460px) {
      --segmented-columns: repeat(2, 1fr);
      --segmented-flow: row;
    }

    @container (inline-size >= 460px) {
      --segmented-columns: repeat(4, 1fr);
      --segmented-flow: row;
    }
  }
</style>
