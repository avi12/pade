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
import type { Agent, AgentSession, SaveMigration, Settings } from "@/lib/types";

/** Whether `dir` is `base` itself or nested anywhere under it (normalized). */
export function isUnderDirectory({ directory, base }: {
  directory: string;
  base: string;
}): boolean {
  const normalizedBase = normalizePath(base);
  const normalized = normalizePath(directory);
  return normalized === normalizedBase || normalized.startsWith(`${normalizedBase}/`);
}

/** Re-point `dir` (a path at or under `from`) to the same suffix under `to`. */
export function remapDirectory({ directory, from, to }: {
  directory: string;
  from: string;
  to: string;
}): string {
  return to + directory.slice(from.length);
}

/** What the app shell provides for a relocation. */
export interface RelocateHost {
  sessions: () => AgentSession[];
  currentProject: () => string;
  /** Mark these sessions as deliberately closing BEFORE they are killed, so the
   *  shell's PTY-exit handler treats their exits as intentional — not as an agent
   *  the user quit. Without this, killing the last agent of an unnamed temp
   *  workspace makes the exit handler discard the whole workspace (delete the
   *  folder + jump to the picker) mid-relocation — the very folder rename is
   *  saving. */
  markClosing: (ids: ReadonlySet<string>) => void;
  /** Drop the killed sessions from tabs/panes and re-point the active one. */
  removeSessions: (ids: ReadonlySet<string>) => void;
  /** Re-point the current project dir after the move. */
  setCurrentProject: (path: string) => void;
  /** Resume one displaced live session on its remapped cwd. */
  relaunch: (options: {
    agent: Agent;
    cwd: string;
    initialPrompt: string;
    split: boolean;
  }) => void;
}

/** Move/rename/delete entry points for one app shell, sharing the lock-handling
 *  flow. */
export function createRelocator(host: RelocateHost) {
  function isUnder({ directory, base }: {
    directory: string;
    base: string;
  }): boolean {
    return isUnderDirectory({
      directory,
      base
    });
  }

  /** Free the folder: kill every session holding it (or a child) as cwd, and
   *  report the ones that were still alive so a caller can resume them. */
  async function releaseLock(from: string) {
    const locking = host.sessions().filter(session => isUnder({
      directory: session.cwd ?? host.currentProject(),
      base: from
    }));
    // Capture the live ones + where they were working, to resume after the move.
    const toResume = locking
      .filter(session => sessionStatus(session.id) !== SessionStatus.enum.exited)
      .map(session => ({
        agent: session.agent,
        oldDirectory: session.cwd ?? host.currentProject()
      }));

    // Claim these exits as deliberate before killing, so the shell doesn't read a
    // killed temp agent as "the user quit" and discard the workspace we're saving.
    const lockingIds = new Set(locking.map(session => session.id));
    host.markClosing(lockingIds);

    for (const session of locking) {
      await pty.kill(session.id);
      dropSessionStatus(session.id);
      dropContext(session.id);
    }

    host.removeSessions(lockingIds);
    return toResume;
  }

  /** One entry the `releaseLock` step captured for later resumption. */
  type ResumableSession = Awaited<ReturnType<typeof releaseLock>>[number];

  /** Resume the live sessions on the new path, seeded to continue; the first
   *  reuses the current pane, the rest split beside it. */
  function resumeSessions({ toResume, from, to }: {
    toResume: ResumableSession[];
    from: string;
    to: string;
  }): void {
    toResume.forEach((entry, index) => host.relaunch({
      agent: entry.agent,
      cwd: remapDirectory({
        directory: entry.oldDirectory,
        from,
        to
      }),
      initialPrompt: "continue\r",
      split: index > 0
    }));
  }

  async function relocate({ from, run }: {
    from: string;
    run: () => Promise<string>;
  }): Promise<string> {
    const toResume = await releaseLock(from);

    // Run the backend move/rename (also re-points every external reference).
    const newPath = await run();
    await workspace.settings();

    if (isUnder({
      directory: host.currentProject(),
      base: from
    })) {
      host.setCurrentProject(
        remapDirectory({
          directory: host.currentProject(),
          from,
          to: newPath
        })
      );
    }

    resumeSessions({
      toResume,
      from,
      to: newPath
    });

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
    /** Destination among the saved roots; omitted → the primary root. */
    root?: string;
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
    if (isUnder({
      directory: host.currentProject(),
      base: path
    })) {
      host.setCurrentProject("");
    }

    return settings;
  }

  /** Save a temp workspace by copy-migration: renaming its folder fails on
   *  Windows while the watcher holds it open, so the backend copies its files
   *  into a fresh saved project instead. Same lock release first (which frees the
   *  temp folder so it can be deleted); then the backend copy; then the live
   *  sessions resume on the new dir and the emptied temp folder is removed. The
   *  install command is handed back for the caller to run in a runner pane. */
  async function saveMigrate(target: {
    from: string;
    newName: string;
    root?: string;
  }): Promise<SaveMigration> {
    const toResume = await releaseLock(target.from);

    const migration = await workspace.saveMigrate(target);
    await workspace.settings();

    if (isUnder({
      directory: host.currentProject(),
      base: target.from
    })) {
      host.setCurrentProject(migration.path);
    }

    resumeSessions({
      toResume,
      from: target.from,
      to: migration.path
    });

    // Best-effort: the temp's agents are dead now so the folder should delete,
    // but the copy is the important work — never let a delete failure sink it.
    await workspace.delete(target.from).catch(() => {});

    return migration;
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
    saveMigrate,
    remove,
    removeDirectory
  };
}
