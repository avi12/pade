import type { LifecycleHost } from "@/panels/picker/lifecycle.svelte";
import { createWorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
import { describe, expect, it, vi } from "vitest";

const PROJECT = "C:\\repositories\\avi\\poll-avi";
const RENAMED = "C:\\repositories\\avi\\poll";

function host(overrides: Partial<LifecycleHost> = {}): LifecycleHost {
  return {
    isOwned: () => false,
    onmove: vi.fn(async () => ""),
    onrename: vi.fn(async () => ""),
    ondelete: vi.fn(),
    onrenamefolder: vi.fn(async () => RENAMED),
    openWindows: vi.fn(async () => []),
    refresh: vi.fn(async () => {}),
    ...overrides
  };
}

describe("folder rename", () => {
  it("renames through the relocator, then refreshes the lists", async () => {
    const lifecycleHost = host();
    const lifecycle = createWorkspaceLifecycle(lifecycleHost);
    const target = {
      from: PROJECT,
      newName: "poll"
    };

    await expect(lifecycle.renameFolder(target)).resolves.toBe(RENAMED);
    expect(lifecycleHost.onrenamefolder).toHaveBeenCalledWith(target);
    expect(lifecycleHost.refresh).toHaveBeenCalledOnce();
  });

  it("tracks which project the prompt is open for", () => {
    const lifecycle = createWorkspaceLifecycle(host());
    lifecycle.startRenameFolder(PROJECT);
    expect(lifecycle.renameFolderTarget).toBe(PROJECT);
    lifecycle.finishRenameFolder();
    expect(lifecycle.renameFolderTarget).toBeNull();
  });

  it("is withheld for a project another window has open", async () => {
    const lifecycle = createWorkspaceLifecycle(
      host({
        openWindows: async () => [{
          label: "second",
          path: PROJECT,
          isCurrent: false
        }]
      })
    );
    expect(lifecycle.canRenameFolder(PROJECT)).toBe(true);
    await lifecycle.loadOpenWindows();
    expect(lifecycle.canRenameFolder(PROJECT)).toBe(false);
  });

  it("keeps the last-known windows when the list can't be read", async () => {
    let isReadable = true;
    const lifecycle = createWorkspaceLifecycle(
      host({
        async openWindows() {
          if (!isReadable) {
            throw new Error("window list unavailable");
          }

          return [{
            label: "second",
            path: PROJECT,
            isCurrent: false
          }];
        }
      })
    );
    await lifecycle.loadOpenWindows();
    isReadable = false;
    await lifecycle.loadOpenWindows();
    expect(lifecycle.canRenameFolder(PROJECT)).toBe(false);
  });
});
