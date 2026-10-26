// Move / rename / delete a workspace with cwd-lock handling. Every one of them
// touches the folder itself - which fails while a live agent or task runner
// holds it as cwd (Windows lock). So they share one opening move: stop the
// runners and kill the sessions under it (remembering the live ones). Move and
// rename then run the backend op (which also re-points every external reference
// - each agent's memory, IDE recents…) and resume each live session's own
// conversation on the new path; one that was mid-turn is told to continue.
// Delete has nothing to resume and the app simply forgets the folder.
// Idle/exited sessions stay closed. The app shell supplies its session list,
// settings sink and relaunch through `RelocateHost`.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { CommandProtocolMap } from "@/lib/ipc/messaging";
import { sendSettingsMessage } from "@/lib/settings/settings-requests";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { SessionStatus } from "@/lib/types";
import type { Agent, AgentSession, ModelChoice, Settings } from "@/lib/types";
import { normalizePath } from "@/lib/util/paths";

/** What a resumed session that was mid-turn is told, so its turn goes on. */
const CONTINUE_PROMPT = "continue\r";

/** Whether `directory` is `base` itself or nested anywhere under it (normalized). */
export function isUnderDirectory({ directory, base }: {
  directory: string;
  base: string;
}) {
  const normalizedBase = normalizePath(base);
  const normalized = normalizePath(directory);
  return normalized === normalizedBase || normalized.startsWith(`${normalizedBase}/`);
}

/** Re-point `directory` (a path at or under `from`) to the same suffix under `to`. */
export function remapDirectory({ directory, from, to }: {
  directory: string;
  from: string;
  to: string;
}) {
  return to + directory.slice(from.length);
}

/** What the app shell provides for a relocation. */
export interface RelocateHost {
  sessions: () => AgentSession[];
  currentProject: () => string;
  /** Mark these sessions as deliberately closing BEFORE they are killed, so the
   *  shell's PTY-exit handler treats their exits as intentional - not as an agent
   *  the user quit. Without this, killing the last agent of an unnamed temp
   *  workspace makes the exit handler jump to the picker mid-relocation, giving
   *  up the very workspace the rename is saving. */
  markClosing: (ids: ReadonlySet<string>) => void;
  /** Stop the window's task runners - they run inside the workspace too. */
  stopRunners: () => Promise<void>;
  /** Drop the killed sessions from tabs/panes and re-point the active one. */
  removeSessions: (ids: ReadonlySet<string>) => void;
  /** Re-point the current project dir after the move. */
  setCurrentProject: (path: string) => Promise<void>;
  /** Resume one displaced live session's conversation on its remapped cwd. */
  relaunch: (options: {
    agent: Agent;
    project: string;
    cwd: string;
    conversationId?: string;
    args: string[];
    modelChoice?: ModelChoice;
    initialPrompt?: string;
    split: boolean;
  }) => void;
}

/** The entry points `createRelocator` gives an app shell. */
export type Relocator = ReturnType<typeof createRelocator>;

/** Move/rename/delete entry points for one app shell, sharing the lock-handling
 *  flow. */
export function createRelocator(host: RelocateHost) {
  function isOpenWorkspaceUnder(base: string) {
    return isUnderDirectory({
      directory: host.currentProject(),
      base
    });
  }

  /** Free the folder: kill every session holding it (or a child) as cwd, and
   *  report the ones that were still alive so a caller can resume them. */
  async function releaseLock(from: string) {
    if (isOpenWorkspaceUnder(from)) {
      await host.stopRunners();
    }

    const locking = host.sessions().filter(session => isUnderDirectory({
      directory: session.cwd ?? session.project,
      base: from
    }));
    // Capture the live ones, their conversation and where they were working, to
    // resume after the move.
    const toResume = locking
      .filter(session => sessionStatus(session.id) !== SessionStatus.enum.exited)
      .map(session => ({
        agent: session.agent,
        project: session.project,
        oldDirectory: session.cwd ?? session.project,
        conversationId: session.conversationId,
        modelChoice: session.modelChoice,
        isMidTurn: sessionStatus(session.id) === SessionStatus.enum.working
      }));

    // Claim these exits as deliberate before killing, so the shell doesn't read a
    // killed temp agent as "the user quit" and leave the workspace we're saving.
    const lockingIds = new Set(locking.map(session => session.id));
    host.markClosing(lockingIds);

    for (const session of locking) {
      await sendMessage(Command.ptyKill, { id: session.id });
    }

    host.removeSessions(lockingIds);
    return toResume;
  }

  /** One entry the `releaseLock` step captured for later resumption. */
  type ResumableSession = Awaited<ReturnType<typeof releaseLock>>[number];

  /** Resume each live session's own conversation on the new path - the agent's
   *  memory of it moved with the folder; the first reuses the current pane, the
   *  rest split beside it. */
  async function resumeSessions({ toResume, from, to }: {
    toResume: ResumableSession[];
    from: string;
    to: string;
  }) {
    for (const [i, entry] of toResume.entries()) {
      const cwd = remapDirectory({
        directory: entry.oldDirectory,
        from,
        to
      });
      const args = await sendMessage(Command.agentResumeArgs, {
        command: entry.agent.command,
        cwd,
        conversationId: entry.conversationId
      });
      const isProjectMoved = isUnderDirectory({
        directory: entry.project,
        base: from
      });
      host.relaunch({
        agent: entry.agent,
        project: isProjectMoved
          ? remapDirectory({
            directory: entry.project,
            from,
            to
          })
          : entry.project,
        cwd,
        conversationId: entry.conversationId,
        args,
        modelChoice: entry.modelChoice,
        initialPrompt: entry.isMidTurn ? CONTINUE_PROMPT : undefined,
        split: i > 0
      });
    }
  }

  async function relocate({ from, run }: {
    from: string;
    run: () => Promise<string>;
  }) {
    const toResume = await releaseLock(from);

    // Run the backend move/rename (also re-points every external reference).
    // When it fails, the folder is where it was: bring its sessions back there.
    const newPath = await runOrResumeInPlace({
      run,
      toResume,
      from
    });
    await sendSettingsMessage(Command.settingsGet);

    if (isOpenWorkspaceUnder(from)) {
      await host.setCurrentProject(
        remapDirectory({
          directory: host.currentProject(),
          from,
          to: newPath
        })
      );
    }

    await resumeSessions({
      toResume,
      from,
      to: newPath
    });

    return newPath;
  }

  async function runOrResumeInPlace({ run, toResume, from }: {
    run: () => Promise<string>;
    toResume: ResumableSession[];
    from: string;
  }) {
    try {
      return await run();
    } catch (error) {
      await resumeSessions({
        toResume,
        from,
        to: from
      });
      throw error;
    }
  }

  function move(target: Parameters<CommandProtocolMap[Command.workspaceMove]>[0]) {
    return relocate({
      from: target.from,
      run: () => sendMessage(Command.workspaceMove, target)
    });
  }

  function rename(target: Parameters<CommandProtocolMap[Command.workspaceRename]>[0]) {
    return relocate({
      from: target.from,
      run: () => sendMessage(Command.workspaceRename, target)
    });
  }

  /** Rename a workspace's folder where it stands - any project, not only a
   *  temp one. */
  function renameFolder(target: Parameters<CommandProtocolMap[Command.workspaceRenameFolder]>[0]) {
    return relocate({
      from: target.from,
      run: () => sendMessage(Command.workspaceRenameFolder, target)
    });
  }

  /** Delete a workspace: same lock release (a running agent would otherwise keep
   *  the folder open and the removal would fail), then remove it. Nothing to
   *  resume, and the shell lets go of it if it was the open project.
   *  `removeFromDisk` is the backend removal - owned-only for `remove`, ungated
   *  for `removeDirectory`. */
  async function deleteVia({ path, removeFromDisk }: {
    path: string;
    removeFromDisk: (path: string) => Promise<Settings>;
  }) {
    await releaseLock(path);
    const settings = await removeFromDisk(path);
    if (isOpenWorkspaceUnder(path)) {
      host.setCurrentProject("");
    }

    return settings;
  }

  /** Delete an ADE-owned workspace directory (the picker's delete). */
  function remove(path: string) {
    return deleteVia({
      path,
      removeFromDisk: removedPath => sendSettingsMessage(Command.workspaceDelete, { path: removedPath })
    });
  }

  /** Delete ANY project directory from disk - the switcher's "Delete directory".
   *  Ungated, so it can remove a real project; the caller confirms first. */
  function removeDirectory(path: string) {
    return deleteVia({
      path,
      removeFromDisk: removedPath => sendSettingsMessage(Command.workspaceDeleteDirectory, { path: removedPath })
    });
  }

  return {
    move,
    rename,
    renameFolder,
    remove,
    removeDirectory
  };
}
