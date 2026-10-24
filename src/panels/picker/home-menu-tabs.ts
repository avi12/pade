import type { IconName } from "@/lib/Icon.svelte";

/** The ways in at the foot of the composer's project menu: start a new
 *  project, open a folder already on disk, or clone a repository. */
export const HomeMenuTab = {
  New: "new",
  Open: "open",
  Clone: "clone"
} as const;
export type HomeMenuTab = typeof HomeMenuTab[keyof typeof HomeMenuTab];

/** The menu's arrow-key lists (`arrowLists`' `data-arrow-list` names): the
 *  project rows, and the New tab's items, which Down from the New tab
 *  steps into. */
export const HomeMenuList = {
  Projects: "projects",
  New: "new"
} as const;

/** The tabs in reading order - the order the arrow keys walk and the panels
 *  slide by. `entersList` names the list Up/Down from the tab steps into. */
export const HOME_MENU_TABS: readonly {
  id: HomeMenuTab;
  label: string;
  icon: IconName;
  entersList?: typeof HomeMenuList[keyof typeof HomeMenuList];
}[] = [
  {
    id: HomeMenuTab.New,
    label: "New",
    icon: "folderPlus",
    entersList: HomeMenuList.New
  },
  {
    id: HomeMenuTab.Open,
    label: "Open",
    icon: "folderOpen"
  },
  {
    id: HomeMenuTab.Clone,
    label: "Clone",
    icon: "git"
  }
];

/** A tab's element id under its menu - what its panel's `aria-labelledby`
 *  names. */
export function homeMenuTabId({ menuId, tab }: {
  menuId: string;
  tab: HomeMenuTab;
}) {
  return `${menuId}-tab-${tab}`;
}

/** A panel's element id under its menu - what its tab's `aria-controls`
 *  names. */
export function homeMenuPanelId({ menuId, tab }: {
  menuId: string;
  tab: HomeMenuTab;
}) {
  return `${menuId}-panel-${tab}`;
}

function tabIndex(tab: HomeMenuTab) {
  return HOME_MENU_TABS.findIndex(({ id }) => id === tab);
}

/** Which way the panels slide when the selection moves from one tab to
 *  another: 1 when the new tab lies after the old (the new panel comes in
 *  from the end side), -1 when before, 0 when it stays put. */
export function tabSlideDirection({ from, to }: {
  from: HomeMenuTab;
  to: HomeMenuTab;
}) {
  return Math.sign(tabIndex(to) - tabIndex(from));
}
