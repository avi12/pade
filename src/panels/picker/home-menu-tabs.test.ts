import {
  HOME_MENU_TABS,
  homeMenuPanelId,
  HomeMenuTab,
  homeMenuTabId,
  tabSlideDirection
} from "@/panels/picker/home-menu-tabs";
import { describe, expect, it } from "vitest";

describe("HOME_MENU_TABS", () => {
  it("reads New, Open, Clone", () => {
    expect(HOME_MENU_TABS.map(({ id }) => id)).toEqual([HomeMenuTab.New, HomeMenuTab.Open, HomeMenuTab.Clone]);
  });
});

describe("homeMenuTabId / homeMenuPanelId", () => {
  it("gives each tab and its panel distinct ids under one menu", () => {
    const ids = HOME_MENU_TABS.flatMap(({ id }) => [
      homeMenuTabId({
        menuId: "menu",
        tab: id
      }),
      homeMenuPanelId({
        menuId: "menu",
        tab: id
      })
    ]);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.every(id => id.startsWith("menu-"))).toBe(true);
  });
});

describe("tabSlideDirection", () => {
  it("slides in from the end side when the new tab lies after the old", () => {
    expect(
      tabSlideDirection({
        from: HomeMenuTab.New,
        to: HomeMenuTab.Clone
      })
    ).toBe(1);
  });

  it("slides in from the start side when the new tab lies before the old", () => {
    expect(
      tabSlideDirection({
        from: HomeMenuTab.Clone,
        to: HomeMenuTab.Open
      })
    ).toBe(-1);
  });

  it("stays put on the tab already selected", () => {
    expect(
      tabSlideDirection({
        from: HomeMenuTab.Open,
        to: HomeMenuTab.Open
      })
    ).toBe(0);
  });
});
