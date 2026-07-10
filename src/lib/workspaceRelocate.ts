// Relocate (move / rename) a workspace with cwd-lock handling. Either op
// fs::renames the folder — which fails while a live agent holds it as cwd
// (Windows lock). So: kill the sessions under it (remembering the live ones),
// run the backend op (which also re-points every external reference — agent
// memory dirs, IDE recents…), then resume the live ones on the new path seeded
// to continue. Idle/exited sessions stay closed. The app shell supplies its
// session list, settings sink and relaunch through `RelocateHost`.

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

/** Move/rename entry points for one app shell, sharing the lock-handling flow. */
export function createRelocator(host: RelocateHost) {
  async function relocate({ from, run }: {
    from: string;
    run: () => Promise<string>;
  }): Promise<string> {
    function isUnder(dir: string): boolean {
      return isUnderDir({
        dir,
        base: from
      });
    }

    const locking = host.sessions().filter(s => isUnder(s.cwd ?? host.currentProject()));
    // Capture the live ones + where they were working, to resume after the move.
    const toResume = locking
      .filter(s => sessionStatus(s.id) !== SessionStatus.enum.exited)
      .map(s => ({
        agent: s.agent,
        oldDir: s.cwd ?? host.currentProject()
      }));

    // Release the lock: kill every session under the dir.
    for (const session of locking) {
      await pty.kill(session.id);
      dropSessionStatus(session.id);
      dropContext(session.id);
    }

    host.removeSessions(new Set(locking.map(s => s.id)));

    // Run the backend move/rename (also re-points every external reference).
    const newPath = await run();
    host.applySettings(await workspace.settings());

    if (isUnder(host.currentProject())) {
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

  return {
    move,
    rename
  };
}
