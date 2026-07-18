<script lang="ts">
  import type { Prefs } from "@/lib/types";

  // Discord Rich Presence toggles. Persistence stays with the parent (the single
  // settings owner) — this section only reflects the prefs and reports changes,
  // exactly like OnLaunchSection exposes its callbacks.
  const { prefs, ondiscordpresence, ondiscordshowproject }: {
    prefs: Prefs;
    ondiscordpresence: (on: boolean) => void;
    ondiscordshowproject: (on: boolean) => void;
  } = $props();

  // Opt-in: presence is off unless explicitly enabled.
  const presenceOn = $derived(prefs.discordPresence === true);
  // Default on once presence itself is enabled.
  const showProject = $derived(prefs.discordShowProject !== false);
</script>

<section class="discord">
  <h2>Discord</h2>
  <label class="check">
    <span class="ck">
      <input checked={presenceOn} onchange={e => ondiscordpresence(e.currentTarget.checked)} type="checkbox" />
      <span class="box" aria-hidden="true">
        <svg fill="none" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7" /></svg>
      </span>
    </span>
    <span>Show my activity on Discord (Playing PADE)</span>
  </label>
  <label class="check" class:disabled={!presenceOn}>
    <span class="ck">
      <input
        checked={showProject}
        disabled={!presenceOn}
        onchange={e => ondiscordshowproject(e.currentTarget.checked)}
        type="checkbox"
      />
      <span class="box" aria-hidden="true">
        <svg fill="none" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7" /></svg>
      </span>
    </span>
    <span>Show the current project’s name</span>
  </label>
  <p class="hint">Requires the Discord desktop app running on this computer.</p>
</section>

<style>
  /* Match OnLaunchSection's tighter 10px row rhythm (vs the shared 12px). */
  .discord {
    gap: 10px;
  }

  .check {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 13px;
    cursor: pointer;

    /* The dependent row dims until presence is on — it has no effect otherwise
       (the checkbox itself is also disabled). */
    &.disabled {
      color: var(--on-surface-variant);
      cursor: default;
    }
  }
</style>
