<script lang="ts">
  // Discord Rich Presence: whether to report at all, and whether to name the
  // project. The second only matters once the first is on, so it waits disabled.
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import Checkbox from "@/lib/ui/Checkbox.svelte";
</script>

<label class="check">
  <Checkbox checked={effective.discordPresence} onchange={checked => updatePrefs({ discordPresence: checked })} />
  <span class="field-text">
    <span class="field-label">Show my activity</span>
    <span class="field-hint">Reports “Playing PADE” on your Discord profile</span>
  </span>
</label>
<label class="check" class:disabled={!effective.discordPresence}>
  <Checkbox
    checked={effective.discordShowProject}
    disabled={!effective.discordPresence}
    onchange={checked => updatePrefs({ discordShowProject: checked })}
  />
  <span class="field-text">
    <span class="field-label">Include the project name</span>
    <span class="field-hint">Adds the open project and its language icon</span>
  </span>
</label>
<p class="field-hint">Requires the Discord desktop app running on this computer</p>
