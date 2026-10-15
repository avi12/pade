// Auto-name a temp workspace once the agent has produced real work. After a
// few distinct files change, ask the agent (or a heuristic) for a friendly
// label and apply it. Fires once per workspace; never blocks or renames on
// disk - the label is display-only, and never replaces a label the workspace
// already has (the backend re-checks that as it saves, so a name the user set
// while the namer was thinking still wins). The app shell provides the current
// project / prefs / settings sink via `AutoNameHost` and owns the mount
// lifecycle; the watcher subscription lives here.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { sendSettingsMessage } from "@/lib/settings/settings-requests";
import { ChangeKind, LabelSource } from "@/lib/types";
import type { ChangeEvent } from "@/lib/types";
import { isTemporaryWorkspace, normalizePath } from "@/lib/util/paths";
import type { UnlistenFn } from "@tauri-apps/api/event";

const DISTINCT_FILES_BEFORE_NAMING = 3;
const LEADING_SLASH = /^\//;

/** The normalized changed path when it carries naming signal for the workspace
 *  - inside the tree and not a dotfile/dot-dir (e.g. .git, .claude) - else
 *  null. Doubles as the distinct-files key. */
export function namingSignal({ projectDirectory, changedPath }: {
  projectDirectory: string;
  changedPath: string;
}) {
  const base = normalizePath(projectDirectory);
  const touched = normalizePath(changedPath);
  if (!touched.startsWith(base)) {
    return null;
  }

  const relative = touched.slice(base.length).replace(LEADING_SLASH, "");
  const isHidden = relative.split("/").some(segment => segment.startsWith("."));
  return relative && !isHidden ? touched : null;
}

/** What the app shell provides. Reads happen per change event, so the checks
 *  always see the current project and prefs. */
export interface AutoNameHost {
  currentProject: () => string;
  /** Whether the user opted out via prefs.autoNameTemp. */
  isOptedOut: () => boolean;
  /** The label already assigned to a path, if any. */
  labelOf: (path: string) => string | undefined;
  /** The active session's agent command, for the agent-CLI namer. */
  activeAgentCommand: () => string;
}

/** The auto-namer for one app shell: `start()` on mount subscribes to the
 *  change feed, `dispose()` on destroy releases it. */
export function createAutoNamer(host: AutoNameHost) {
  const touchedByProject = new Map<string, Set<string>>();
  const namedProjects = new Set<string>();
  let unlisten: UnlistenFn | undefined;

  function isNamingCandidate(project: string) {
    const isAlreadyNamed = namedProjects.has(project) || Boolean(host.labelOf(project));
    return isTemporaryWorkspace(project) && !host.isOptedOut() && !isAlreadyNamed;
  }

  /** Record `touched` for `project`; how many distinct paths it has seen so far. */
  function recordTouch({ project, touched }: {
    project: string;
    touched: string;
  }) {
    const touchedPaths = touchedByProject.get(project) ?? new Set<string>();
    touchedPaths.add(touched);
    touchedByProject.set(project, touchedPaths);
    return touchedPaths.size;
  }

  /** The namer's suggestion, or null when it fails - a failed suggestion just
   *  leaves the workspace on its temporary name. */
  async function suggestName(project: string) {
    try {
      return await sendMessage(Command.projectAutoname, {
        path: project,
        agent: host.activeAgentCommand()
      });
    } catch {
      return null;
    }
  }

  async function consider(event: ChangeEvent) {
    const project = host.currentProject();
    if (event.kind === ChangeKind.enum.deleted || !isNamingCandidate(project)) {
      return;
    }

    const touched = namingSignal({
      projectDirectory: project,
      changedPath: event.path
    });
    if (!touched) {
      return;
    }

    const distinctTouched = recordTouch({
      project,
      touched
    });
    if (distinctTouched < DISTINCT_FILES_BEFORE_NAMING) {
      return;
    }

    namedProjects.add(project); // guard so the naming call runs exactly once
    const name = await suggestName(project);
    if (!name || host.currentProject() !== project) {
      return;
    }

    await sendSettingsMessage(Command.workspaceSetLabel, {
      path: project,
      name,
      source: LabelSource.enum.auto
    });
  }

  // Subscribe to the change feed. The watcher itself is armed on the open project
  // by the shell - `windowServices` sends `watch_start` whenever the window's
  // project changes - so the namer only listens
  // for the changes it produces; it no longer arms on the (drift-prone) cwd.
  async function start() {
    unlisten = await onMessage(EventType.feedChange, ({ data }) => {
      consider(data);
    });
  }

  function dispose() {
    unlisten?.();
  }

  return {
    start,
    dispose
  };
}
