// Saving a temp workspace as a real project, without a dead gap in its tab.
// The flow lives here (SoC); App.svelte lends it the session actions it owns.

import { workspace } from "@/lib/bridge";
import { successorPrompt } from "@/lib/stores/agent/handoff.svelte";
import { startRunner } from "@/lib/stores/tasks/runners.svelte";
import { TaskGroup } from "@/lib/types";
import type { Agent, AgentSession } from "@/lib/types";

const INSTALL_RUNNER_LABEL = "Install dependencies";

/** What the shell does on the save flow's behalf. */
interface SaveTempHost {
  /** The temp workspace being saved. */
  project: string;
  /** Every session open in it. */
  sessionIds: readonly string[];
  /** The session whose agent carries on in the saved project. */
  continuing: AgentSession | undefined;
  writeHandoffDoc: (session: AgentSession) => Promise<string | null>;
  /** Point the window at the saved project. */
  reroot: (path: string) => void;
  launch: (options: {
    agent: Agent;
    cwd: string;
    initialPrompt?: string;
  }) => void;
  /** Kill and detach these sessions as a deliberate close. */
  closeSessions: (ids: readonly string[]) => Promise<void>;
}

export async function saveTempWorkspace({ name, root, host }: {
  name: string;
  root?: string;
  host: SaveTempHost;
}) {
  // 1. Have the live agent dump a handoff doc INTO THE TEMP first, so it rides
  //    along in the copy and the successor continues from it.
  const handoffDoc = host.continuing ? await host.writeHandoffDoc(host.continuing) : null;

  // 2. Copy the temp workspace (handoff doc included) into the saved root while
  //    its agent keeps running - a copy only READS files, so it never trips the
  //    watcher/cwd handle a rename does.
  const migration = await workspace.saveMigrate({
    from: host.project,
    newName: name,
    root
  });

  // 3. Re-root the window to the saved project, then start its agent in the
  //    SAME tab pointing at the new dir (launch replaces the panes), seeded to
  //    continue the handoff, BEFORE tearing the temp session down - so the tab
  //    transitions straight to the successor with no dead gap. The old session
  //    is then killed and the temp deleted.
  host.reroot(migration.path);

  if (host.continuing) {
    host.launch({
      agent: host.continuing.agent,
      cwd: migration.path,
      initialPrompt: handoffDoc ? successorPrompt(handoffDoc) : undefined
    });
  }

  await host.closeSessions(host.sessionIds);
  // The project already lives on in its saved copy, so a temp that can't be
  // deleted right now is simply left in place.
  await Promise.allSettled([workspace.delete(host.project)]);

  if (migration.install) {
    await startRunner({
      label: INSTALL_RUNNER_LABEL,
      kind: TaskGroup.shape.kind.enum.npm,
      command: migration.install,
      cwd: migration.path
    });
  }
}
