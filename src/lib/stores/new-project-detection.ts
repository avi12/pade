// One-shot project-evidence coordination for newly created workspaces. The
// filesystem event arms a cheap marker probe; project-kind and editor stores
// remain responsible for their own cached state.

import { feed } from "@/lib/bridge";
import { refreshEditors } from "@/lib/stores/editors.svelte";
import { refreshProjectKind } from "@/lib/stores/projectKinds.svelte";
import { ChangeKind } from "@/lib/types";
import { isUnderDirectory } from "@/lib/workspace-relocate";
import type { UnlistenFn } from "@tauri-apps/api/event";

const MARKER_SETTLE_MS = 300;

const newProjects = new Set<string>();

const markerTimers = new Map<string, ReturnType<typeof setTimeout>>();
let unlistenEvidence: UnlistenFn | undefined;

async function detectNewProjectMarker(project: string): Promise<void> {
  markerTimers.delete(project);
  const kind = await refreshProjectKind(project);
  if (!kind) {
    return;
  }

  newProjects.delete(project);
  await refreshEditors(project);
}

function scheduleMarkerDetection(project: string): void {
  const previous = markerTimers.get(project);
  if (previous !== undefined) {
    clearTimeout(previous);
  }

  markerTimers.set(
    project, setTimeout(async () => {
      await detectNewProjectMarker(project);
    }, MARKER_SETTLE_MS)
  );
}

/** Subscribe once to filesystem changes. Only newly-created workspaces are armed;
 * their cheap registry marker probe retires permanently after its first match. */
export async function initNewProjectDetection(): Promise<void> {
  if (unlistenEvidence) {
    return;
  }

  try {
    unlistenEvidence = await feed.onChange(event => {
      if (event.kind !== ChangeKind.enum.created) {
        return;
      }

      for (const project of newProjects) {
        if (isUnderDirectory({
          directory: event.path,
          base: project
        })) {
          scheduleMarkerDetection(project);
        }
      }
    });
  } catch {
    // Project launch remains usable, and a later initializer can retry.
    unlistenEvidence = undefined;
  }
}

export function armNewProjectDetection(project: string): void {
  newProjects.add(project);
}

export function disposeNewProjectDetection(): void {
  unlistenEvidence?.();
  unlistenEvidence = undefined;
  for (const timer of markerTimers.values()) {
    clearTimeout(timer);
  }
  markerTimers.clear();
  newProjects.clear();
}
