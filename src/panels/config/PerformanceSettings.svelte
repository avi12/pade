<script lang="ts">
  // Software rendering, which only takes effect on a fresh start - so the
  // restart sits right beside it.
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import Checkbox from "@/lib/ui/Checkbox.svelte";
</script>

<div class="field-row">
  <label class="check">
    <Checkbox checked={effective.softwareRender} onchange={checked => updatePrefs({ softwareRender: checked })} />
    <span class="field-text">
      <span class="field-label">Software rendering</span>
      <span class="field-hint">Frees the GPU for games. Applies after restart</span>
    </span>
  </label>
  <button
    class="restart"
    onclick={async () => await sendMessage(Command.restart)}
    type="button"
  >Restart PADE</button>
</div>

<style>
  .restart {
    flex: none;
    padding-block: 7px;
    padding-inline: 12px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font: inherit;
    font-weight: 700;
    font-size: 11px;
    cursor: pointer;
    transition: filter 140ms var(--ease);

    &:hover {
      filter: brightness(0.96);
    }
  }
</style>
