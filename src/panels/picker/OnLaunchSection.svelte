<script lang="ts">
  import { globalNaming } from "@/lib/agent/ai-naming-scope";
  import { effective } from "@/lib/settings/prefs.svelte";
  import type { Prefs, StartMode } from "@/lib/types";
  import ChoiceRow from "@/panels/picker/ChoiceRow.svelte";
  import ExplorerContextMenuToggle from "@/panels/picker/ExplorerContextMenuToggle.svelte";
  import { START_MODE_CHOICES, TEMP_CLEANUP_CHOICES } from "@/panels/picker/on-launch";
  import ToggleRow from "@/panels/picker/ToggleRow.svelte";

  // "On launch" preferences: the no-project start mode, unused-temp cleanup,
  // temp auto-naming, and the Windows Explorer "Open in PADE" context-menu
  // toggle. Pref persistence reaches shared settings through the parent; the
  // context-menu toggle is self-contained (ExplorerContextMenuToggle).
  const { prefs, onstartmode, ontempcleanup, onautoname, onainaming }: {
    prefs: Prefs;
    onstartmode: (mode: StartMode) => void;
    ontempcleanup: (days: number | null) => void;
    onautoname: (on: boolean) => void;
    onainaming: (on: boolean) => void;
  } = $props();

  const startMode = $derived(effective.startMode);
  const tempCleanupDays = $derived(prefs.tempCleanupDays ?? null);
  const autoName = $derived(prefs.autoNameTemp !== false);
  const aiNaming = $derived(globalNaming(prefs));
</script>

<section class="onlaunch">
  <h2>On launch</h2>
  <ChoiceRow caption="With no project, open" choices={START_MODE_CHOICES} onpick={onstartmode} selected={startMode} />
  <ChoiceRow
    caption="Delete temp workspaces unused for"
    choices={TEMP_CLEANUP_CHOICES}
    onpick={ontempcleanup}
    selected={tempCleanupDays}
  />
  <ToggleRow
    caption="Auto-name temp workspaces once the agent starts working"
    checked={autoName}
    onchange={onautoname}
  />
  <ToggleRow caption="Name session tabs with AI, in every project" checked={aiNaming} onchange={onainaming} />
  <p class="scope-hint">A single project can opt in or out of this in its Config panel</p>
  <ExplorerContextMenuToggle />
</section>

<style>
  /* Canon spaces this section's rows 10px apart, a touch tighter than the
     shared 12px picker-section gap. */
  .onlaunch {
    gap: 10px;
  }

  /* Sits under the checkbox it qualifies, indented past the box so it reads as
     that row's footnote rather than a fourth setting. */
  .scope-hint {
    margin-block: -2px 0;
    margin-inline-start: 30px;
    color: var(--on-surface-variant);
    font-size: 12px;
  }
</style>
