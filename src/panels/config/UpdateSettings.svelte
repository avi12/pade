<script lang="ts">
  // PADE's self-update (`stores/update/appUpdate`): the running version, the
  // switch that lets it check GitHub, and - once a newer release is downloaded -
  // the way to restart into it now rather than on the next close. An MSIX
  // install is Windows' to update, so the switch and Check now stay disabled.
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import { appUpdate, checkForUpdate, restartToUpdate } from "@/lib/stores/update/appUpdate.svelte";
  import { Updater } from "@/lib/types";
  import Checkbox from "@/lib/ui/controls/Checkbox.svelte";
  import { updateStatusLine } from "@/lib/update/status-line";

  const status = $derived(appUpdate.status);
  const isUpdatable = $derived(status?.updater === Updater.enum.pade);
  const ready = $derived(status?.ready ?? null);
</script>

<div class="field-row">
  <label class="check" class:disabled={!isUpdatable}>
    <Checkbox
      checked={effective.autoUpdate}
      disabled={!isUpdatable}
      onchange={checked => updatePrefs({ autoUpdate: checked })}
    />
    <span class="field-text">
      <span class="field-label">Check for updates</span>
      <span class="field-hint">Looks for a newer release on GitHub every few hours and downloads it</span>
    </span>
  </label>
  {#if ready}
    <button class="field-action" onclick={async () => await restartToUpdate()} type="button">Restart to update</button>
  {:else}
    <button
      class="field-action"
      disabled={!isUpdatable || appUpdate.isChecking}
      onclick={async () => isUpdatable && await checkForUpdate()}
      type="button"
    >Check now</button>
  {/if}
</div>

<p class="field-hint">{updateStatusLine(status)}</p>

<style>
  p {
    margin: 0;
  }
</style>
