// The window's one task-catalog snapshot. It owns backend discovery, manifest
// change refreshes, stale-request rejection, and errors; every consumer reads
// the same accepted TaskGroup[] rather than fetching or reconstructing it.

import type { UnlistenFn } from "@tauri-apps/api/event";
import { untrack } from "svelte";

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { frontProject } from "@/lib/stores/workspace/active-project/frontProject.svelte";
import type { ChangeEvent } from "@/lib/types/feed";
import { ManifestMatch } from "@/lib/types/terminal";
import type { TaskGroup, TaskManifestDescriptor } from "@/lib/types/terminal";
import { baseName } from "@/lib/util/paths";

export type TaskCatalogSnapshot = Readonly<{
  project: string;
  groups: TaskGroup[];
  descriptors: TaskManifestDescriptor[];
  error: string | null;
}>;

interface TaskCatalogDependencies {
  /** The project whose tasks the catalog lists - the window's front one. */
  project: () => string;
  descriptors: () => Promise<TaskManifestDescriptor[]>;
  list: (project: string) => Promise<TaskGroup[]>;
  /** A watched file changed - in any project the window holds. */
  onChange: (callback: (change: Pick<ChangeEvent, "root" | "path">) => void) => Promise<UnlistenFn>;
}

const defaultDependencies: TaskCatalogDependencies = {
  project: frontProject,
  descriptors: () => sendMessage(Command.TASKS_DESCRIPTORS),
  list: project => sendMessage(Command.TASKS_LIST, { cwd: project }),
  onChange: callback => onMessage(EventType.FEED_CHANGE, ({ data }) => callback(data))
};

/** Whether a changed path is one of the manifests the backend reads - an exact
 *  file name, or a file carrying a manifest extension. Case-insensitive, like
 *  the backend's own match: a `.CSPROJ` is still a project. */
function isManifestPath({ descriptors, path }: { descriptors: TaskManifestDescriptor[]; path: string }) {
  const name = baseName(path).toLowerCase();
  return descriptors.some(descriptor => {
    const value = descriptor.value.toLowerCase();
    return descriptor.match === ManifestMatch.enum.name ? name === value : name.endsWith(value);
  });
}

export function createTaskCatalog(dependencies: TaskCatalogDependencies = defaultDependencies) {
  let snapshot = $state<TaskCatalogSnapshot>({
    project: "",
    groups: [],
    descriptors: [],
    error: null
  });

  let requestVersion = 0;
  let unlisten: UnlistenFn | undefined;
  let initialization: Promise<void> | undefined;
  let initializationError: string | null = null;

  const currentProject = dependencies.project;

  async function refresh(project = currentProject()) {
    const version = ++requestVersion;
    if (!project) {
      snapshot = {
        ...snapshot,
        project,
        groups: [],
        error: initializationError
      };
      return;
    }

    try {
      const groups = await dependencies.list(project);
      const isStaleResponseOnRefresh = version !== requestVersion || project !== currentProject();
      if (isStaleResponseOnRefresh) {
        return;
      }

      snapshot = {
        ...snapshot,
        project,
        groups,
        error: initializationError
      };
    } catch (caughtError) {
      const isStaleResponse = version !== requestVersion || project !== currentProject();
      if (isStaleResponse) {
        return;
      }

      snapshot = {
        ...snapshot,
        project,
        groups: [],
        error: String(caughtError)
      };
    }
  }

  async function refreshAfterManifestChange() {
    await refresh();
  }

  async function start() {
    try {
      const descriptors = await dependencies.descriptors();
      const nextUnlisten = await dependencies.onChange(({ root, path }) => {
        const isListedManifest =
          root === currentProject() &&
          isManifestPath({
            descriptors,
            path
          });
        if (isListedManifest) {
          void refreshAfterManifestChange();
        }
      });
      snapshot = {
        ...snapshot,
        descriptors
      };
      initializationError = null;
      unlisten = nextUnlisten;
    } catch (caughtError) {
      initializationError = String(caughtError);
      snapshot = {
        ...snapshot,
        error: initializationError
      };
    }
  }

  async function initialize() {
    if (!unlisten) {
      initialization ??= start();

      await initialization;
      initialization = undefined;
    }

    await refresh();
  }

  return {
    get snapshot() {
      return snapshot;
    },
    initialize,
    refresh
  };
}

export const taskCatalog = createTaskCatalog();

// The window's catalog follows its front project: each one that comes to the
// front is scanned afresh.
$effect.root(() => {
  $effect(() => {
    const project = frontProject();
    void untrack(() => taskCatalog.refresh(project));
  });
});
