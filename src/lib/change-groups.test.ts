import { groupChanges, GroupRole } from "@/lib/change-groups";
import type { ChangeEvent } from "@/lib/types";
import { describe, expect, it } from "vitest";

let idCounter = 0;
function change({ path, added = 0, removed = 0, timestamp = 0 }: {
  path: string;
  added?: number;
  removed?: number;
  timestamp?: number;
}): ChangeEvent {
  idCounter += 1;
  return {
    id: `e${idCounter}`,
    path,
    kind: "modified",
    added,
    removed,
    summary: `Edited ${path}`,
    ts: timestamp
  };
}

const ROOT = "C:/repositories/avi/pade";

describe("groupChanges", () => {
  it("puts a single-project repo's changes in one group named after the repo", () => {
    const groups = groupChanges({
      workspaceRoot: ROOT,
      events: [
        change({ path: `${ROOT}/src/App.svelte` }),
        change({ path: `${ROOT}/src-tauri/src/watcher.rs` })
      ]
    });
    expect(groups).toHaveLength(1);
    expect(groups[0]).toMatchObject({
      id: ".",
      name: "pade",
      role: GroupRole.App
    });
    expect(groups[0].events).toHaveLength(2);
  });

  it("splits a monorepo into member groups with roles from the container", () => {
    const groups = groupChanges({
      workspaceRoot: ROOT,
      events: [
        change({
          path: `${ROOT}/apps/desktop/tests/meter.test.ts`,
          added: 31
        }),
        change({
          path: `${ROOT}/packages/hooks/src/useUsage.ts`,
          added: 19
        }),
        change({
          path: `${ROOT}/services/api/usage.py`,
          added: 12,
          removed: 3
        })
      ]
    });
    const byId = Object.fromEntries(groups.map(group => [group.id, group]));
    expect(byId["apps/desktop"]).toMatchObject({
      name: "desktop",
      role: GroupRole.App
    });
    expect(byId["packages/hooks"]).toMatchObject({
      name: "hooks",
      role: GroupRole.Lib
    });
    expect(byId["services/api"]).toMatchObject({
      name: "api",
      role: GroupRole.Service
    });
  });

  it("sums line deltas and event counts within a group", () => {
    const [group] = groupChanges({
      workspaceRoot: ROOT,
      events: [
        change({
          path: `${ROOT}/packages/hooks/a.ts`,
          added: 19
        }),
        change({
          path: `${ROOT}/packages/hooks/b.ts`,
          added: 7,
          removed: 4
        })
      ]
    });
    expect(group).toMatchObject({
      added: 26,
      removed: 4
    });
    expect(group.events).toHaveLength(2);
  });

  it("keeps an @scope/name member whole", () => {
    const [group] = groupChanges({
      workspaceRoot: ROOT,
      events: [change({ path: `${ROOT}/packages/@pade/theme/src/theme.css` })]
    });
    expect(group).toMatchObject({
      id: "packages/@pade/theme",
      name: "@pade/theme"
    });
  });

  it("buckets a root-level file into the repo group, not a member", () => {
    const groups = groupChanges({
      workspaceRoot: ROOT,
      events: [
        change({ path: `${ROOT}/README.md` }),
        change({ path: `${ROOT}/packages/hooks/src/index.ts` })
      ]
    });
    const ids = groups.map(group => group.id).sort();
    expect(ids).toEqual([".", "packages/hooks"]);
  });

  it("orders groups by their most-recent change", () => {
    const groups = groupChanges({
      workspaceRoot: ROOT,
      events: [
        change({
          path: `${ROOT}/packages/old/x.ts`,
          timestamp: 100
        }),
        change({
          path: `${ROOT}/packages/fresh/y.ts`,
          timestamp: 500
        })
      ]
    });
    expect(groups.map(group => group.name)).toEqual(["fresh", "old"]);
  });

  it("matches the root prefix case-insensitively (Windows drive casing)", () => {
    const [group] = groupChanges({
      workspaceRoot: "C:/Repos/App",
      events: [change({ path: "c:/repos/app/packages/core/index.ts" })]
    });
    expect(group.id).toBe("packages/core");
  });
});
