// The window's one task-catalog snapshot. It owns backend discovery, manifest
// change refreshes, stale-request rejection, and errors; every consumer reads
// the same accepted TaskGroup[] rather than fetching or reconstructing it.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { frontProject } from "@/lib/stores/workspace/frontProject.svelte";
import { ManifestMatch } from "@/lib/types";
import type { ChangeEvent, TaskGroup, TaskManifestDescriptor } from "@/lib/types";
import { baseName } from "@/lib/util/paths";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { untrack } from "svelte";

export type TaskCatalogSnapshot = Readonly<{
  project: string;
  groups: TaskGroup[];
  descriptors: TaskManifestDescriptor[];
  error: string | null;
}>;

type TaskCatalogDependencies = {
  /** The project whose tasks the catalog lists - the window's front one. */
  project: () => string;
  descriptors: () => Promise<TaskManifestDescriptor[]>;
  list: (project: string) => Promise<TaskGroup[]>;
  /** A watched file changed - in any project the window holds. */
  onChange: (callback: (change: Pick<ChangeEvent, "root" | "path">) => void) => Promise<UnlistenFn>;
};

const defaultDependencies: TaskCatalogDependencies = {
  project: frontProject,
  descriptors: () => sendMessage(Command.tasksDescriptors),
  list: project => sendMessage(Command.tasksList, { cwd: project }),
  onChange: callback => onMessage(EventType.feedChange, ({ data }) => callback(data))
};

/** Whether a changed path is one of the manifests the backend reads - an exact
 *  file name, or a file carrying a manifest extension. Case-insensitive, like
 *  the backend's own match: a `.CSPROJ` is still a project. */
function isManifestPath({ descriptors, path }: {
  descriptors: TaskManifestDescriptor[];
  path: string;
}) {
  const name = baseName(path).toLowerCase();
  return descriptors.some(descriptor => {
    const value = descriptor.value.toLowerCase();
    return descriptor.match === ManifestMatch.enum.name ? name === value : name.endsWith(value);
  });
}

export function createTaskCatalog(
  dependencies: TaskCatalogDependencies = defaultDependencies
) {
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
      if (version !== requestVersion || project !== currentProject()) {
        return;
      }

      snapshot = {
        ...snapshot,
        project,
        groups,
        error: initializationError
      };
    } catch (caughtError) {
      if (version !== requestVersion || project !== currentProject()) {
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
        const isListedManifest = root === currentProject() && isManifestPath({
          descriptors,
          path
        });
        if (isListedManifest) {
          refreshAfterManifestChange();
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
    untrack(() => taskCatalog.refresh(project));
  });
});
