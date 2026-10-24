<script lang="ts">
  // PADE's self-update (`stores/update/appUpdate`): the running version, the
  // switch that lets it check GitHub, and - once a newer release is downloaded -
  // the way to restart into it now rather than on the next close.
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import { appUpdate, checkForUpdate, restartToUpdate } from "@/lib/stores/update/appUpdate.svelte";
  import Checkbox from "@/lib/ui/Checkbox.svelte";

  const status = $derived(appUpdate.status);
  const isUpdatable = $derived(status?.isSupported === true);
  const ready = $derived(status?.ready ?? null);
  const statusLine = $derived.by(() => {
    if (!status) {
      return "Reading the version…";
    }

    if (!isUpdatable) {
      return `PADE ${status.current} · a development build, which never updates itself`;
    }

    if (ready) {
      return `PADE ${status.current} · ${ready} is ready and installs when PADE closes`;
    }

    return `PADE ${status.current} · up to date`;
  });
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

<p class="field-hint">{statusLine}</p>

<style>
  p {
    margin: 0;
  }
</style>
