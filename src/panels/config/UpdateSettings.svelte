<script lang="ts">
  // PADE's release notice (`stores/update/appUpdate`): the running version, the
  // switch that lets it check GitHub, and - once a newer release exists - the
  // way to open its page and install the MSIX package. Only an unpackaged copy
  // checks: an MSIX install is Windows' to update, so the switch and Check now
  // stay disabled there.
  import { openLink } from "@/lib/dashboard/open-link";
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import { appUpdate, checkForUpdate } from "@/lib/stores/update/appUpdate.svelte";
  import { Updater } from "@/lib/types/update";
  import Checkbox from "@/lib/ui/controls/Checkbox.svelte";
  import { updateStatusLine } from "@/lib/update/status-line";

  const status = $derived(appUpdate.status);
  const isUpdatable = $derived(status?.updater === Updater.enum.unpackaged);
  const newer = $derived(status?.newer ?? null);
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
      <span class="field-hint">Looks for a newer release on GitHub every few hours</span>
    </span>
  </label>
  {#if newer}
    <button class="field-action" onclick={async () => await openLink(newer.releaseUrl)} type="button">Open release page</button>
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
