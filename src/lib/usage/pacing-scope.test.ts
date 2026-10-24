import { PacedWindowKind, Settings } from "@/lib/types";
import type { Prefs } from "@/lib/types";
import { DEFAULT_PACING, PacingPreset, withPreset } from "@/lib/usage/pacing-policy";
import {
  globalPacing,
  projectWindowPacing,
  resolvedPacing,
  withGlobalWindowPacing,
  withProjectWindowPacing
} from "@/lib/usage/pacing-scope";
import { describe, expect, it } from "vitest";

const PROJECT = "C:/work/app";
const STRICT_SESSION = withPreset({
  settings: DEFAULT_PACING.session,
  preset: PacingPreset.Strict
});
const OFF_WEEKLY = withPreset({
  settings: DEFAULT_PACING.weekly,
  preset: PacingPreset.Off
});

describe("resolvedPacing", () => {
  it("runs on the built-in defaults when nothing is set", () => {
    expect(
      resolvedPacing({
        prefs: {},
        project: PROJECT
      })
    ).toEqual(DEFAULT_PACING);
  });

  it("lets a workspace override one window and follow the global for the other", () => {
    const prefs: Prefs = {
      pacing: withGlobalWindowPacing({
        prefs: {},
        kind: PacedWindowKind.enum.session,
        settings: STRICT_SESSION
      }),
      pacingProjects: withProjectWindowPacing({
        prefs: {},
        project: PROJECT,
        kind: PacedWindowKind.enum.weekly,
        settings: OFF_WEEKLY
      })
    };
    expect(
      resolvedPacing({
        prefs,
        project: PROJECT
      })
    ).toEqual({
      session: STRICT_SESSION,
      weekly: OFF_WEEKLY
    });
    expect(
      resolvedPacing({
        prefs,
        project: "C:/work/other"
      }).weekly
    ).toEqual(DEFAULT_PACING.weekly);
  });

  it("drops a workspace's override, and the workspace once it overrides nothing", () => {
    const prefs: Prefs = {
      pacingProjects: withProjectWindowPacing({
        prefs: {},
        project: PROJECT,
        kind: PacedWindowKind.enum.weekly,
        settings: OFF_WEEKLY
      })
    };
    const cleared = withProjectWindowPacing({
      prefs,
      project: PROJECT,
      kind: PacedWindowKind.enum.weekly,
      settings: undefined
    });
    expect(cleared).toEqual({});
    expect(
      projectWindowPacing({
        prefs: {
          pacingProjects: cleared
        },
        project: PROJECT,
        kind: PacedWindowKind.enum.weekly
      })
    ).toBeUndefined();
  });
});

describe("stored pacing", () => {
  it("reads settings saved before the split as the session window's", () => {
    const { prefs } = Settings.parse({
      roots: [],
      defaultAgent: null,
      projectAgents: {},
      prefs: {
        pacing: STRICT_SESSION
      }
    });
    expect(globalPacing(prefs)).toEqual({
      session: STRICT_SESSION,
      weekly: DEFAULT_PACING.weekly
    });
  });
});
