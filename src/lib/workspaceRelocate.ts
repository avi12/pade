// Move / rename / delete a workspace with cwd-lock handling. Every one of them
// touches the folder itself — which fails while a live agent holds it as cwd
// (Windows lock). So they share one opening move: kill the sessions under it
// (remembering the live ones). Move and rename then run the backend op (which
// also re-points every external reference — agent memory dirs, IDE recents…)
// and resume the live sessions on the new path, seeded to continue; delete has
// nothing to resume and the app simply forgets the folder. Idle/exited sessions
// stay closed. The app shell supplies its session list, settings sink and
// relaunch through `RelocateHost`.

import { pty, workspace } from "@/lib/bridge";
import { normalizePath } from "@/lib/paths";
import { dropContext } from "@/lib/stores/context.svelte";
import { dropSessionStatus, sessionStatus } from "@/lib/stores/sessions.svelte";
import { SessionStatus } from "@/lib/types";
import type { Agent, AgentSession, Settings } from "@/lib/types";

/** Whether `dir` is `base` itself or nested anywhere under it (normalized). */
export function isUnderDir({ dir, base }: {
  dir: string;
  base: string;
}): boolean {
  const normalizedBase = normalizePath(base);
  const normalized = normalizePath(dir);
  return normalized === normalizedBase || normalized.startsWith(`${normalizedBase}/`);
}

/** Re-point `dir` (a path at or under `from`) to the same suffix under `to`. */
export function remapDir({ dir, from, to }: {
  dir: string;
  from: string;
  to: string;
}): string {
  return to + dir.slice(from.length);
}

/** What the app shell provides for a relocation. */
export interface RelocateHost {
  sessions: () => AgentSession[];
  currentProject: () => string;
  /** Drop the killed sessions from tabs/panes and re-point the active one. */
  removeSessions: (ids: ReadonlySet<string>) => void;
  /** Apply the refreshed settings after the backend op ran. */
  applySettings: (settings: Settings) => void;
  /** Re-point the current project dir after the move. */
  setCurrentProject: (path: string) => void;
  /** Resume one displaced live session on its remapped cwd. */
  relaunch: (opts: {
    agent: Agent;
    cwd: string;
    initialPrompt: string;
    split: boolean;
  }) => void;
}

/** Move/rename/delete entry points for one app shell, sharing the lock-handling
 *  flow. */
export function createRelocator(host: RelocateHost) {
  function isUnder({ dir, base }: {
    dir: string;
    base: string;
  }): boolean {
    return isUnderDir({
      dir,
      base
    });
  }

  /** Free the folder: kill every session holding it (or a child) as cwd, and
   *  report the ones that were still alive so a caller can resume them. */
  async function releaseLock(from: string) {
    const locking = host.sessions().filter(s => isUnder({
      dir: s.cwd ?? host.currentProject(),
      base: from
    }));
    // Capture the live ones + where they were working, to resume after the move.
    const toResume = locking
      .filter(s => sessionStatus(s.id) !== SessionStatus.enum.exited)
      .map(s => ({
        agent: s.agent,
        oldDir: s.cwd ?? host.currentProject()
      }));

    for (const session of locking) {
      await pty.kill(session.id);
      dropSessionStatus(session.id);
      dropContext(session.id);
    }

    host.removeSessions(new Set(locking.map(s => s.id)));
    return toResume;
  }

  async function relocate({ from, run }: {
    from: string;
    run: () => Promise<string>;
  }): Promise<string> {
    const toResume = await releaseLock(from);

    // Run the backend move/rename (also re-points every external reference).
    const newPath = await run();
    host.applySettings(await workspace.settings());

    if (isUnder({
      dir: host.currentProject(),
      base: from
    })) {
      host.setCurrentProject(
        remapDir({
          dir: host.currentProject(),
          from,
          to: newPath
        })
      );
    }

    // Resume the live sessions on the new path, seeded to continue.
    toResume.forEach((entry, index) => host.relaunch({
      agent: entry.agent,
      cwd: remapDir({
        dir: entry.oldDir,
        from,
        to: newPath
      }),
      initialPrompt: "continue\r",
      split: index > 0
    }));

    return newPath;
  }

  function move(target: {
    from: string;
    destDir: string;
  }): Promise<string> {
    return relocate({
      from: target.from,
      run: () => workspace.move(target)
    });
  }

  function rename(target: {
    from: string;
    newName: string;
  }): Promise<string> {
    return relocate({
      from: target.from,
      run: () => workspace.rename(target)
    });
  }

  /** Delete a workspace: same lock release (a running agent would otherwise keep
   *  the folder open and the removal would fail), then remove it. Nothing to
   *  resume, and the shell lets go of it if it was the open project. `del` is the
   *  backend removal — owned-only for `remove`, ungated for `removeDirectory`. */
  async function deleteVia(path: string, del: (path: string) => Promise<Settings>): Promise<Settings> {
    await releaseLock(path);
    const settings = await del(path);
    host.applySettings(settings);

    if (isUnder({
      dir: host.currentProject(),
      base: path
    })) {
      host.setCurrentProject("");
    }

    return settings;
  }

  /** Delete an ADE-owned workspace directory (the picker's delete). */
  function remove(path: string): Promise<Settings> {
    return deleteVia(path, workspace.delete);
  }

  /** Delete ANY project directory from disk — the switcher's "Delete directory".
   *  Ungated, so it can remove a real project; the caller confirms first. */
  function removeDirectory(path: string): Promise<Settings> {
    return deleteVia(path, workspace.deleteDirectory);
  }

  return {
    move,
    rename,
    remove,
    removeDirectory
  };
}
