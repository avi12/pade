<script lang="ts">
  import RelabelDialog from "@/lib/dialogs/RelabelDialog.svelte";
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import type { AddRootOutcome, WindowInfo } from "@/lib/types";
  import { arrowFocus } from "@/lib/ui/roving-menu";
  import { ToggleState } from "@/lib/ui/toggle-state";
  import SaveWorkspaceCard from "@/lib/workspace/save/SaveWorkspaceCard.svelte";
  import { windowRowsInOrder } from "@/lib/workspace/switcher/switcher-rows";
  import SwitcherTrigger from "@/lib/workspace/switcher/SwitcherTrigger.svelte";
  import { runListTransition } from "@/lib/workspace/switcher/windows/list-transition";
  import OpenWindowList from "@/lib/workspace/switcher/windows/OpenWindowList.svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";

  // The top bar's project chip and its window menu. Switching projects lives in
  // the left rail; this menu keeps what has no other home: saving a throwaway
  // workspace as a real project, and every open PADE window (jump between them,
  // drag them into the Ctrl+Alt+[ / ] cycle order, or merge this window into
  // one - its sessions move there, still running). Up/Down walk the window
  // rows: every stop is tagged `data-arrow-stop`, and `arrowFocus` moves focus
  // between them.
  const { path, label, isTemp, remoteRepository, roots, labels, onsavetemp, onaddroot, onmerge }: {
    path: string;
    label: string;
    isTemp: boolean;
    /** The repository the trigger's Ctrl-click opens on its remote; null when
     *  it has none. */
    remoteRepository: string | null;
    /** The saved project roots - destinations the temp workspace can be saved into. */
    roots: string[];
    labels: Record<string, string>;
    /** Save the current temp workspace as a real project: rename it into `root`
     *  (one of the saved roots) under `name`. Resolves once relocated. */
    onsavetemp: (target: {
      name: string;
      root: string;
    }) => Promise<void>;
    /** Add an existing folder as a project root - the no-roots save path.
     *  Persisted by the parent into shared settings. */
    onaddroot: (path: string) => Promise<AddRootOutcome>;
    /** Move every session of this window into the window `windowLabel`
     *  (still running there), then close this one. */
    onmerge: (windowLabel: string) => Promise<void>;
  } = $props();

  const MENU_ID = "application-menu";
  const MENU_ANCHOR = "--application-menu-anchor";
  const POPOVER_OPEN_SELECTOR = ":popover-open";
  const ARROW_STOP_SELECTOR = "[data-arrow-stop]";

  // The open-window list is fetched when the menu opens.
  let windowRows = $state<WindowInfo[]>([]);
  // Whether the popover is open - the save card prefills its name then.
  let isMenuOpen = $state(false);
  // The rows container, so a reorder animates through a view transition scoped
  // to *just* that element (never the document - that would snapshot the
  // live-repainting terminal and ghost it).
  let elWindowList = $state<HTMLElement | null>(null);
  // The relabel prompt nests inside this menu so it stays visible behind it.
  let isRelabeling = $state(false);
  // The relabel prompt starts from the workspace's PADE-only label, if any.
  const currentLabel = $derived(labels[path] ?? "");

  async function loadOpenWindows() {
    try {
      windowRows = await sendMessage(Command.windowList);
    } catch (error) {
      console.warn("pade: keeping the last-known open windows", error);
    }
  }

  function hide() {
    const elMenu = document.getElementById(MENU_ID);
    if (elMenu?.matches(POPOVER_OPEN_SELECTOR)) {
      elMenu.hidePopover();
    }
  }

  // Load once up front so the menu never opens on an empty "Open windows" head;
  // each open re-fetches.
  onMount(loadOpenWindows);

  // Reflect open-windows changes made in OTHER windows live (their reorder lands
  // → this menu morphs to match), so two open menus never disagree.
  let unlistenWindowsChanged: UnlistenFn | undefined;
  onMount(async () => {
    unlistenWindowsChanged = await onMessage(EventType.windowsChanged, async () => {
      if (!isMenuOpen) {
        return;
      }

      let openWindows: WindowInfo[];
      try {
        openWindows = await sendMessage(Command.windowList);
      } catch {
        return;
      }

      await runListTransition({
        elList: elWindowList,
        async mutate() {
          windowRows = openWindows;
          await tick();
        }
      });
    });
  });
  onDestroy(() => unlistenWindowsChanged?.());
</script>

<span class="menu-host">
  <SwitcherTrigger {isTemp} {label} menuAnchor={MENU_ANCHOR} menuId={MENU_ID} {remoteRepository} />

  <div
    id={MENU_ID}
    style:position-anchor={MENU_ANCHOR}
    class="menu popover-menu" {@attach arrowFocus(ARROW_STOP_SELECTOR)}
    ontoggle={async e => {
      isMenuOpen = (e as ToggleEvent).newState === ToggleState.Open;

      if (isMenuOpen) {
        await loadOpenWindows();
      }
    }}
    popover
    role="menu"
  >
    <!-- Save the throwaway workspace as a real project: name + destination root.
         Design's "Save this workspace" card, shown only for a temp project. -->
    {#if isTemp}
      <SaveWorkspaceCard
        {isMenuOpen}
        {label}
        {onaddroot}
        onrelabel={() => {
          isRelabeling = true;
        }}
        onsaved={hide}
        {onsavetemp}
        {roots}
      />
      <div class="menu-divider"></div>
    {/if}

    <OpenWindowList
      {labels}
      onfocuswindow={async windowLabel => {
        hide();
        await sendMessage(Command.windowFocusLabel, { label: windowLabel });
      }}
      onmerge={async windowLabel => {
        hide();
        await onmerge(windowLabel);
      }}
      onreorder={async windowLabels => {
        // Persist the new order to the one backend source (which drives both
        // this list and the Ctrl+Alt+[ / ] cycle), reflect it at once, then
        // re-fetch so the rows match backend truth.
        windowRows = windowRowsInOrder({
          windowRows,
          order: windowLabels
        });
        await sendMessage(Command.windowReorder, { labels: windowLabels });
        await loadOpenWindows();
      }}
      {windowRows}
      bind:elList={elWindowList}
    />

    {#if isRelabeling}
      <RelabelDialog
        {currentLabel}
        nested
        onclose={() => {
          isRelabeling = false;
        }}
        {path}
      />
    {/if}
  </div>
</span>

<style>
  .menu-host {
    display: contents;
  }

  /* Shell comes from the shared .popover-menu; width, colour and anchor side here. */
  .menu {
    inline-size: 352px;
    max-inline-size: 92vw;
    color: var(--on-surface);
    animation: pop-in 220ms var(--spring);
    position-area: bottom span-right;
  }
</style>
