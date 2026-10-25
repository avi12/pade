<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { OpenTarget } from "@/lib/types";
  import { ToggleState } from "@/lib/ui/controls/toggle-state";
  import { rovingTablist } from "@/lib/ui/keyboard/roving-tabs";
  import ClonePanel from "@/panels/picker/ClonePanel.svelte";
  import {
    HOME_MENU_TABS,
    homeMenuPanelId,
    HomeMenuTab,
    homeMenuTabId,
    tabSlideDirection
  } from "@/panels/picker/home-menu-tabs";
  import HomeMenuTabPanel from "@/panels/picker/HomeMenuTabPanel.svelte";
  import HomeNewSection from "@/panels/picker/HomeNewSection.svelte";
  import OpenLocalPanel from "@/panels/picker/OpenLocalPanel.svelte";

  // The composer menu's foot, under the scrolling project list: three ways in
  // behind a tablist - New (a project under a root, or a throwaway), Open (a
  // folder already on disk) and Clone (a repository). Open and Clone end the
  // same way a project row does: the folder becomes the composer's project
  // and the menu closes, so Start opens it with the typed prompt and agent.
  let {
    menuId,
    roots,
    isTempPicked,
    isSeparated,
    onnew,
    ontemp,
    onpick,
    onnewroot,
    createIn = $bindable("")
  }: {
    menuId: string;
    roots: string[];
    isTempPicked: boolean;
    isSeparated: boolean;
    onnew: (root: string) => void;
    ontemp: () => void;
    /** A folder opened or cloned here becomes the composer's project. */
    onpick: (path: string) => void;
    onnewroot: () => void;
    createIn?: string;
  } = $props();

  let selectedTab = $state<HomeMenuTab>(HomeMenuTab.New);
  // Which way the last switch moved, so the panels slide the same way.
  let slideDirection = $state(0);

  // Close the menu - unless it already closed (a clone can outlast the menu
  // being dismissed).
  function hideMenu() {
    const elMenu = document.getElementById(menuId);
    if (elMenu?.matches(":popover-open")) {
      elMenu.hidePopover();
    }
  }

  // A closed menu forgets the last switch, so reopening it shows the
  // selected panel in place: the slide and the height blend belong to
  // switching tabs, not to the menu opening.
  function forgetSlideOnClose() {
    const elMenu = document.getElementById(menuId);
    if (!elMenu) {
      return;
    }

    const listening = new AbortController();
    elMenu.addEventListener("toggle", e => {
      if (e.newState === ToggleState.Closed) {
        slideDirection = 0;
      }
    }, { signal: listening.signal });
    return () => listening.abort();
  }

  // Shared by Open and Clone: the folder becomes the composer's project.
  function pickFolder({ path }: OpenTarget) {
    onpick(path);
    hideMenu();
  }
</script>

<div class="footer">
  {#if isSeparated}
    <hr class="menu-divider" />
  {/if}
  <div class="pill-tabs" aria-label="Start from" role="tablist" use:rovingTablist>
    {#each HOME_MENU_TABS as { id, label, icon, entersList } (id)}
      <button
        id={homeMenuTabId({
          menuId,
          tab: id
        })}
        class="pill-tab"
        aria-controls={homeMenuPanelId({
          menuId,
          tab: id
        })}
        aria-selected={selectedTab === id}
        data-arrow-enters={entersList}
        data-arrow-open-focus={selectedTab === id || undefined}
        onclick={() => {
          slideDirection = tabSlideDirection({
            from: selectedTab,
            to: id
          });
          selectedTab = id;
        }}
        role="tab"
        tabindex={selectedTab === id ? 0 : -1}
        type="button"
      >
        <Icon name={icon} size={14} />
        {label}
      </button>
    {/each}
  </div>

  <div style:--tab-slide-direction={slideDirection} class="panels" {@attach forgetSlideOnClose}>
    <HomeMenuTabPanel isForm={false} isSelected={selectedTab === HomeMenuTab.New} {menuId} tab={HomeMenuTab.New}>
      <HomeNewSection {isTempPicked} {menuId} {onnew} {ontemp} {roots} />
    </HomeMenuTabPanel>
    <HomeMenuTabPanel isForm isSelected={selectedTab === HomeMenuTab.Open} {menuId} tab={HomeMenuTab.Open}>
      <OpenLocalPanel isActive={selectedTab === HomeMenuTab.Open} onopen={pickFolder} />
    </HomeMenuTabPanel>
    <HomeMenuTabPanel isForm isSelected={selectedTab === HomeMenuTab.Clone} {menuId} tab={HomeMenuTab.Clone}>
      <ClonePanel
        onnewroot={() => {
          hideMenu();
          onnewroot();
        }}
        onopen={pickFolder}
        {roots}
        bind:createIn
      />
    </HomeMenuTabPanel>
  </div>
</div>

<style>
  /* The menu's non-scrolling foot (HomeProjectMenu lays the menu out as a
     column): it keeps its full size while the list above it scrolls. */
  .footer {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: 6px;

    /* The rule parting the tabs from the project list; the foot's own gap
       already spaces it from the tabs. */
    .menu-divider {
      margin-block-end: 0;
    }

    /* The menu box lets clicks through (HomeProjectMenu); the tabs and the
       shown panel (HomeMenuTabPanel) take them back. */
    .pill-tabs {
      margin-inline: 4px;
      pointer-events: auto;
    }
  }

  /* Every panel (HomeMenuTabPanel) shares this grid's one cell and slides
     the way the selection last moved; the clip keeps the slide inside. */
  .panels {
    display: grid;
    overflow: clip;
  }
</style>
