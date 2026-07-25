// The window's one task-catalog snapshot. It owns backend discovery, manifest
// change refreshes, stale-request rejection, and errors; every consumer reads
// the same accepted TaskGroup[] rather than fetching or reconstructing it.

import { feed, tasks } from "@/lib/bridge";
import { baseName } from "@/lib/paths";
import type { TaskGroup, TaskManifestDescriptor } from "@/lib/types";
import type { UnlistenFn } from "@tauri-apps/api/event";

export type TaskCatalogSnapshot = Readonly<{
  project: string;
  groups: TaskGroup[];
  descriptors: TaskManifestDescriptor[];
  error: string | null;
}>;

type TaskCatalogDependencies = {
  descriptors: () => Promise<TaskManifestDescriptor[]>;
  list: (project: string) => Promise<TaskGroup[]>;
  onChange: (callback: (path: string) => void) => Promise<UnlistenFn>;
};

const defaultDependencies: TaskCatalogDependencies = {
  descriptors: tasks.descriptors,
  list: tasks.list,
  onChange: callback => feed.onChange(event => callback(event.path))
};

export function createTaskCatalog(
  dependencies: TaskCatalogDependencies = defaultDependencies
) {
  let snapshot = $state<TaskCatalogSnapshot>({
    project: "",
    groups: [],
    descriptors: [],
    error: null
  });

  function snapshotProject(): string {
    return snapshot.project;
  }

  let projectGetter: () => string = snapshotProject;
  let requestVersion = 0;
  let unlisten: UnlistenFn | undefined;
  let initialization: Promise<void> | undefined;
  let initializationError: string | null = null;

  function currentProject(): string {
    return projectGetter();
  }

  async function refresh(project = currentProject()): Promise<void> {
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

  async function refreshAfterManifestChange(): Promise<void> {
    await refresh();
  }

  async function start(): Promise<void> {
    try {
      const descriptors = await dependencies.descriptors();
      const manifestFiles = descriptors.map(descriptor => descriptor.file);
      const nextUnlisten = await dependencies.onChange(path => {
        if (manifestFiles.includes(baseName(path))) {
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

  async function initialize(nextProjectGetter: () => string): Promise<void> {
    projectGetter = nextProjectGetter;

    if (!unlisten) {
      if (!initialization) {
        initialization = start();
      }

      await initialization;
      initialization = undefined;
    }

    await refresh();
  }

  return {
    get snapshot(): TaskCatalogSnapshot {
      return snapshot;
    },
    initialize,
    refresh
  };
}

export const taskCatalog = createTaskCatalog();
