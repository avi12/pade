<script lang="ts">
  import { contextMenu } from "@/lib/bridge";
  import { StartMode } from "@/lib/types";
  import type { Prefs } from "@/lib/types";
  import { onMount } from "svelte";

  // "On launch" preferences: the no-project start mode, temp auto-naming, and
  // the Windows Explorer "Open in PADE" context-menu toggle. Pref persistence
  // stays with the parent (single settings owner); the context-menu toggle is
  // self-contained (per-user registry via the bridge, no settings involved).
  const { prefs, onstartmode, onautoname }: {
    prefs: Prefs;
    onstartmode: (mode: StartMode) => void;
    onautoname: (on: boolean) => void;
  } = $props();

  const startMode = $derived(prefs.startMode ?? StartMode.enum.temp);
  const autoName = $derived(prefs.autoNameTemp !== false);

  // Explorer "Open in PADE" folder context menu (Windows-only, per-user).
  // `null` until the registry answers — the row waits for it so the checkbox
  // is born showing the real state instead of flipping a beat after paint.
  const isWindows = navigator.userAgent.includes("Windows");
  let ctxMenuOn = $state<boolean | null>(null);
  // Surfaced when registering the modern (Win11) menu fails — typically because
  // Developer Mode is off. The legacy menu still gets added in that case.
  let ctxMenuError = $state("");
  async function loadCtxMenu() {
    if (isWindows) {
      ctxMenuOn = await contextMenu.status();
    }
  }
  onMount(() => void loadCtxMenu());
</script>

<section class="onlaunch">
  <h2>On launch</h2>
  <div class="startmode">
    <span class="sm-label">With no project, open</span>
    <div class="sm-toggle" role="tablist">
      <button
        class="sm-btn"
        class:on={startMode === StartMode.enum.temp}
        aria-selected={startMode === StartMode.enum.temp}
        onclick={() => onstartmode(StartMode.enum.temp)}
        role="tab"
      >Temp workspace</button>
      <button
        class="sm-btn"
        class:on={startMode === StartMode.enum.picker}
        aria-selected={startMode === StartMode.enum.picker}
        onclick={() => onstartmode(StartMode.enum.picker)}
        role="tab"
      >This picker</button>
    </div>
  </div>
  <label class="check">
    <span class="ck">
      <input checked={autoName} onchange={e => onautoname(e.currentTarget.checked)} type="checkbox" />
      <span class="box" aria-hidden="true">
        <svg fill="none" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7" /></svg>
      </span>
    </span>
    <span>Auto-name temp workspaces once the agent starts working</span>
  </label>
  {#if isWindows && ctxMenuOn !== null}
    <label class="check">
      <span class="ck">
        <input
          checked={ctxMenuOn}
          onchange={async e => {
            const on = e.currentTarget.checked;
            ctxMenuError = "";
            try {
              if (on) {
                await contextMenu.register();
              } else {
                await contextMenu.unregister();
              }
            } catch (err) {
              ctxMenuError = err instanceof Error ? err.message : String(err);
            }
            ctxMenuOn = await contextMenu.status();
          }}
          type="checkbox"
        />
        <span class="box" aria-hidden="true">
          <svg fill="none" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7" /></svg>
        </span>
      </span>
      <span>Add “Open in PADE” to the folder right-click menu</span>
    </label>
    {#if ctxMenuError}
      <p class="ctx-error" role="alert">{ctxMenuError}</p>
    {/if}
  {/if}
</section>

<style>
  /* ── On launch — segmented toggle + checkboxes. ── */
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

  /* Pill segmented toggle. */
  .sm-toggle {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-2);

    .sm-btn {
      padding: 6px 14px;
      border: none;
      border-radius: 999px;
      background: transparent;
      color: var(--on-surface-variant);
      font: inherit;
      font-weight: 600;
      font-size: 12px;
      cursor: pointer;
      transition:
        background 150ms var(--ease),
        color 150ms var(--ease);

      &.on {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }
  }

  .check {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 13px;
    cursor: pointer;
  }

  /* Modern-menu registration failure (e.g. Developer Mode off). A tonal warning
     surface rather than a hard border, per M3. */
  .ctx-error {
    margin-block: 2px 0;
    margin-inline-start: 30px;
    padding: 8px 12px;
    border-radius: 12px;
    background: var(--warning-wash);
    color: var(--warning);
    font-size: 12px;
    line-height: 1.4;
  }
</style>
