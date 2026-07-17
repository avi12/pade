<script lang="ts">
  import { ide, os } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import type { Ide } from "@/lib/types";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";

  // Trailing kebab + actions popover for one project row: reveal in Files /
  // Terminal / editor, and — for PADE-owned workspaces — the rename / move /
  // delete lifecycle. Chrome comes from picker/chrome.css.
  const { path, scope, ides, lifecycle }: {
    path: string;
    /** Disambiguates a path shown in more than one section (Recent AND under
     *  its root) — the same path would otherwise mint duplicate popover ids
     *  and clicking one kebab would open the wrong menu. */
    scope: string;
    ides: Ide[];
    lifecycle: WorkspaceLifecycle;
  } = $props();

  // Stable, valid popover id/anchor per row.
  const identifier = $derived(`menu-${scope}-${path.replaceAll(/[^a-zA-Z0-9]/g, "-")}`);

  // The picker has no terminal, so it can only open a project in a GUI editor
  // (a console editor needs a TTY — that's launched from the workspace instead).
  const guiEditor = $derived(ides.find(editor => !editor.terminal));
</script>

<button
  style:anchor-name="--{identifier}"
  class="kebab"
  aria-label="Row actions"
  popovertarget={identifier}
><Icon name="more" /></button>
<ul id={identifier} style:position-anchor="--{identifier}" class="menu popover-menu" popover>
  <li class="head">Reveal</li>
  <li>
    <button class="mi" onclick={() => void os.explorer(path)} popovertarget={identifier} popovertargetaction="hide">
      <Icon name="folder" /><span class="mi-txt">Open in Files</span>
    </button>
  </li>
  <li>
    <button class="mi" onclick={() => void os.terminal(path)} popovertarget={identifier} popovertargetaction="hide">
      <Icon name="terminal" /><span class="mi-txt">Open in Terminal</span>
    </button>
  </li>
  {#if guiEditor}
    <li>
      <button
        class="mi"
        onclick={() => void ide.open({
          command: guiEditor.command,
          path
        })}
        popovertarget={identifier}
        popovertargetaction="hide"
      >
        <Icon name="code" /><span class="mi-txt">Open in {guiEditor.label}</span>
      </button>
    </li>
  {/if}
  {#if lifecycle.isOwned(path)}
    <li class="head sep">Workspace</li>
    <li>
      <button
        class="mi"
        onclick={() => lifecycle.startRename(path)}
        popovertarget={identifier}
        popovertargetaction="hide"
      >
        <Icon name="pencil" /><span class="mi-txt">Rename to a project</span>
      </button>
    </li>
    <li>
      <button
        class="mi"
        onclick={async () => await lifecycle.moveWorkspace(path)}
        popovertarget={identifier}
        popovertargetaction="hide"
      >
        <Icon name="swap" /><span class="mi-txt">Move…</span>
      </button>
    </li>
    <li>
      <!-- Shift-click is the power-user path: it skips the confirmation and
           deletes straight away (a failure still surfaces in the dialog). -->
      <button
        class="mi danger"
        onclick={async e => {
          if (e.shiftKey) {
            await lifecycle.deleteNow(path);
            return;
          }

          lifecycle.requestDelete(path);
        }}
        popovertarget={identifier}
        popovertargetaction="hide"
      >
        <Icon name="trash" /><span class="mi-txt">Delete workspace</span><kbd>⇧</kbd>
      </button>
    </li>
  {/if}
</ul>
