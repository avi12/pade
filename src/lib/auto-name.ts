// Auto-name a temp workspace once the agent has produced real work. After a
// few distinct files change, ask the agent (or a heuristic) for a friendly
// label and apply it. Fires once per workspace; never blocks or renames on
// disk — the label is display-only. The app shell provides the current
// project / prefs / settings sink via `AutoNameHost` and owns the mount
// lifecycle; the watcher subscription lives here.

import { feed, workspace } from "@/lib/bridge";
import { isTemporaryWorkspace, normalizePath } from "@/lib/paths";
import { ChangeKind } from "@/lib/types";
import type { ChangeEvent, Settings } from "@/lib/types";
import type { UnlistenFn } from "@tauri-apps/api/event";

const AUTONAME_AFTER = 3;

/** The normalized changed path when it carries naming signal for the workspace
 *  — inside the tree and not a dotfile/dot-dir (e.g. .git, .claude) — else
 *  null. Doubles as the distinct-files key. */
export function namingSignal({ projectDir, changedPath }: {
  projectDir: string;
  changedPath: string;
}): string | null {
  const base = normalizePath(projectDir);
  const touched = normalizePath(changedPath);
  if (!touched.startsWith(base)) {
    return null;
  }

  const relative = touched.slice(base.length).replace(/^\//, "");
  if (!relative || relative.split("/").some(segment => segment.startsWith("."))) {
    return null;
  }

  return touched;
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
  /** Apply the refreshed settings once the label is stored. */
  applySettings: (settings: Settings) => void;
}

/** The auto-namer for one app shell: `start()` on mount subscribes to the
 *  change feed, `dispose()` on destroy releases it. */
export function createAutoNamer(host: AutoNameHost) {
  const touchedByProject = new Map<string, Set<string>>();
  const namedProjects = new Set<string>();
  let unlisten: UnlistenFn | undefined;

  async function consider(event: ChangeEvent) {
    const project = host.currentProject();
    if (!isTemporaryWorkspace(project) || host.isOptedOut()) {
      return;
    }

    const alreadyNamed = namedProjects.has(project) || Boolean(host.labelOf(project));
    if (event.kind === ChangeKind.enum.deleted || alreadyNamed) {
      return;
    }

    const touched = namingSignal({
      projectDir: project,
      changedPath: event.path
    });
    if (!touched) {
      return;
    }

    const set = touchedByProject.get(project) ?? new Set<string>();
    set.add(touched);
    touchedByProject.set(project, set);

    if (set.size < AUTONAME_AFTER) {
      return;
    }

    namedProjects.add(project); // guard so the naming call runs exactly once
    const name = await workspace.autoname({
      path: project,
      agent: host.activeAgentCommand()
    }).catch(() => null);
    if (name && host.currentProject() === project) {
      host.applySettings(
        await workspace.setLabel({
          path: project,
          name
        })
      );
    }
  }

  // Subscribe to the change feed. The watcher itself is armed on the open project
  // by the Change Feed panel — `ChangeFeed` calls `feed.start(project)`, and it is
  // the default side panel whenever a project is open — so the namer only listens
  // for the changes it produces; it no longer arms on the (drift-prone) cwd.
  async function start() {
    unlisten = await feed.onChange(event => void consider(event));
  }

  function dispose() {
    unlisten?.();
  }

  return {
    start,
    dispose
  };
}
