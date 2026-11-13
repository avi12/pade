<script lang="ts">
  import type { IconName } from "@/lib/icon-name";
  import { chooseThemeMode, chooseThemePalette, effective } from "@/lib/settings/prefs.svelte";
  import { ThemeMode, ThemePalette } from "@/lib/types/theme";
  import Segmented from "@/lib/ui/controls/Segmented.svelte";
  import FontSizeStepper from "@/panels/config/appearance/FontSizeStepper.svelte";
  import HandoffThreshold from "@/panels/config/appearance/HandoffThreshold.svelte";
  import TerminalColours from "@/panels/config/appearance/TerminalColours.svelte";
  import TerminalFontPicker from "@/panels/config/appearance/TerminalFontPicker.svelte";

  // The Appearance card's fields: the light/dark mode and the skin over it,
  // terminal font and colours, UI zoom and the handoff threshold - each bound to
  // a persisted pref via updatePrefs (merge → apply → save through the shared
  // store).
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
    }
  ] as const satisfies readonly {
    value: ThemeMode;
    label: string;
    icon: IconName;
  }[];

  const skinOptions = [
    {
      value: ThemePalette.enum.default,
      label: "Default",
      icon: "spark"
    },
    {
      value: ThemePalette.enum.cyberpunk,
      label: "Cyberpunk",
      icon: "cyberpunk"
    }
  ] as const satisfies readonly {
    value: ThemePalette;
    label: string;
    icon: IconName;
  }[];
</script>

<div class="field">
  <span class="field-label">Theme</span>
  <Segmented
    chosen={effective.themeMode}
    label="Theme"
    onchoose={chooseThemeMode}
    options={themeOptions}
  />
</div>

<div class="field">
  <span class="field-label">Skin</span>
  <Segmented
    chosen={effective.themePalette}
    label="Skin"
    onchoose={chooseThemePalette}
    options={skinOptions}
  />
  <span class="skin-hint">Works with Light, Dark and Auto</span>
</div>

<TerminalFontPicker />

<TerminalColours />

<FontSizeStepper />

<HandoffThreshold />

<style>
  /* The design sets this one hint a size up from the card's `.field-hint`. */
  .skin-hint {
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }
</style>
