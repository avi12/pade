import { isDashboardAddress, windowQueryFor, WindowRoute, windowRouteFor } from "@/lib/shell/window-route";
import { describe, expect, it } from "vitest";

const PROJECT = "C:\\repositories\\avi\\pade & friends";

describe("windowRouteFor", () => {
  it("boots a temp window into a throwaway workspace", () => {
    expect(windowRouteFor("?w=temp")).toEqual({ kind: WindowRoute.Temp });
  });

  it("boots an empty window into the picker", () => {
    expect(windowRouteFor("?w=empty")).toEqual({ kind: WindowRoute.Picker });
  });

  it("boots an open window into its project", () => {
    expect(windowRouteFor(`?w=open&path=${encodeURIComponent(PROJECT)}`)).toEqual({
      kind: WindowRoute.Project,
      path: PROJECT
    });
  });

  it("boots a hand-off window into its project to take the sessions handed to it", () => {
    expect(windowRouteFor(`?w=handoff&path=${encodeURIComponent(PROJECT)}`)).toEqual({
      kind: WindowRoute.Handoff,
      path: PROJECT
    });
    expect(windowRouteFor("?w=handoff")).toEqual({ kind: WindowRoute.LaunchContext });
  });

  it("falls back to the launch context for an open window without a usable path", () => {
    expect(windowRouteFor("?w=open")).toEqual({ kind: WindowRoute.LaunchContext });
    expect(windowRouteFor("?w=open&path=%20%20")).toEqual({ kind: WindowRoute.LaunchContext });
  });

  it("falls back to the launch context without a known mode", () => {
    expect(windowRouteFor("")).toEqual({ kind: WindowRoute.LaunchContext });
    expect(windowRouteFor("?w=sideways")).toEqual({ kind: WindowRoute.LaunchContext });
  });
});

describe("windowQueryFor", () => {
  it("addresses a window without a project as the picker", () => {
    expect(windowQueryFor("")).toBe("?w=empty");
    expect(windowRouteFor(windowQueryFor(""))).toEqual({ kind: WindowRoute.Picker });
  });

  it("addresses a window by its project so a reload reopens it", () => {
    expect(windowRouteFor(windowQueryFor(PROJECT))).toEqual({
      kind: WindowRoute.Project,
      path: PROJECT
    });
  });
});

describe("isDashboardAddress", () => {
  it("tells the dashboard window from every other", () => {
    expect(isDashboardAddress("?w=dashboard")).toBe(true);
    expect(isDashboardAddress("?w=empty")).toBe(false);
    expect(isDashboardAddress("")).toBe(false);
  });
});
