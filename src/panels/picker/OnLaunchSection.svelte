<script lang="ts">
  import { globalNaming } from "@/lib/agent/ai-naming-scope";
  import { StartMode, TEMP_CLEANUP_DAY_CHOICES } from "@/lib/types";
  import type { Prefs } from "@/lib/types";
  import { rovingTablist } from "@/lib/ui/roving-tabs";
  import ExplorerContextMenuToggle from "@/panels/picker/ExplorerContextMenuToggle.svelte";
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

  const startMode = $derived(prefs.startMode ?? StartMode.enum.temp);
  const tempCleanupDays = $derived(prefs.tempCleanupDays ?? null);
  const autoName = $derived(prefs.autoNameTemp !== false);
  const aiNaming = $derived(globalNaming(prefs));
</script>

<section class="onlaunch">
  <h2>On launch</h2>
  <div class="startmode">
    <span class="sm-label">With no project, open</span>
    <div class="pill-tabs" role="tablist" use:rovingTablist>
      <button
        class="pill-tab"
        aria-selected={startMode === StartMode.enum.temp}
        onclick={() => onstartmode(StartMode.enum.temp)}
        role="tab"
        tabindex={startMode === StartMode.enum.temp ? 0 : -1}
      >Temp workspace</button>
      <button
        class="pill-tab"
        aria-selected={startMode === StartMode.enum.picker}
        onclick={() => onstartmode(StartMode.enum.picker)}
        role="tab"
        tabindex={startMode === StartMode.enum.picker ? 0 : -1}
      >This picker</button>
    </div>
  </div>
  <div class="startmode">
    <span class="sm-label">Delete temp workspaces unused for</span>
    <div class="pill-tabs" role="tablist" use:rovingTablist>
      {#each TEMP_CLEANUP_DAY_CHOICES as days (days)}
        <button
          class="pill-tab"
          aria-selected={tempCleanupDays === days}
          onclick={() => ontempcleanup(days)}
          role="tab"
          tabindex={tempCleanupDays === days ? 0 : -1}
        >
          {#if days === null}
            Off{:else}{days} days{/if}
        </button>
      {/each}
    </div>
  </div>
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

  /* ── On launch - checkboxes; the pill-tabs chrome is shared (chrome.css). ── */
  .startmode {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }

  .sm-label {
    color: var(--on-surface-variant);
    font-size: 13px;
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
