<script lang="ts">
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { setExplorerContextMenu } from "@/panels/picker/explorer-context-menu";
  import ToggleRow from "@/panels/picker/ToggleRow.svelte";
  import { onMount } from "svelte";

  // Explorer "Open in PADE" folder context menu (Windows-only, per-user). Self-
  // contained: the per-user registry via the bridge, no settings involved.
  // `null` until the registry answers - the row waits for it so the checkbox
  // is born showing the real state instead of flipping a beat after paint.
  const isWindows = navigator.userAgent.includes("Windows");
  let ctxMenuOn = $state<boolean | null>(null);
  // Surfaced when registering the modern (Win11) menu fails - typically because
  // Developer Mode is off. The legacy menu still gets added in that case.
  let contextMenuError = $state("");
  const isRowShown = $derived(isWindows && ctxMenuOn !== null);

  onMount(async () => {
    if (isWindows) {
      ctxMenuOn = await sendMessage(Command.contextMenuStatus);
    }
  });
</script>

{#if isRowShown}
  <ToggleRow
    caption="Add “Open in PADE” to the folder right-click menu"
    checked={ctxMenuOn === true}
    onchange={async on => {
      contextMenuError = "";
      contextMenuError = await setExplorerContextMenu(on);
      // Always re-read the registry - it's the real answer.
      ctxMenuOn = await sendMessage(Command.contextMenuStatus);
    }}
  />
  {#if contextMenuError}
    <p class="context-menu-error" role="alert">{contextMenuError}</p>
  {/if}
{/if}

<style>
  /* Modern-menu registration failure (e.g. Developer Mode off). A tonal warning
     surface rather than a hard border, per M3. */
  .context-menu-error {
    margin-block: 2px 0;
    margin-inline-start: 30px;
    padding: 8px 12px;
    border-radius: 12px;
    background: var(--warning-wash);
    color: var(--warning);
    font-size: 0.75rem;
    line-height: 1.4;
  }
</style>
