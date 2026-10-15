// The repository the top bar points at - the workspace root, or the nested
// checkout the member switcher selected - and what its branch pill shows: the
// local branches (which also enable per-branch agents), the branches only the
// remote has so far, the checked-out branch and whether a remote exists. A
// workspace with one member reads exactly as a plain project.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { activeMemberIn } from "@/lib/stores/workspace/workspaceMembers.svelte";
import { settledValue } from "@/lib/util/settled";

export function createActiveRepository({ project }: {
  /** The window's open project; empty while none is. */
  project: () => string;
}) {
  // Local branches when the project is a git repo - enables per-branch agents.
  let branches = $state<string[]>([]);
  // Branches only the remote has so far - offered by the branch switcher, which
  // lets git create the local tracking branch on the way in.
  let remoteBranches = $state<string[]>([]);
  // The checked-out branch - the branch pill (empty when the project isn't a git
  // repo or HEAD is detached).
  let currentBranch = $state("");
  // Whether the repo has a remote - a local-only repo has nothing to open in a
  // browser, so the pill's Ctrl-click-opens-on-remote affordance is suppressed.
  let hasRemote = $state(false);

  function clear() {
    branches = [];
    remoteBranches = [];
    currentBranch = "";
    hasRemote = false;
  }

  async function load() {
    const member = activeMemberIn(project());
    if (!member) {
      clear();
      return;
    }

    // Not a git repository (or git missing) reads as no branches and no remote.
    const [next, onlyRemote, heads, remote] = await Promise.all([
      settledValue({
        promise: sendMessage(Command.vcsBranches, { cwd: member }),
        fallback: []
      }),
      settledValue({
        promise: sendMessage(Command.vcsRemoteBranches, { cwd: member }),
        fallback: []
      }),
      settledValue<Record<string, string>>({
        promise: sendMessage(Command.vcsBranchOf, { paths: [member] }),
        fallback: {}
      }),
      settledValue<string | null>({
        promise: sendMessage(Command.vcsRemoteUrl, { cwd: member }),
        fallback: null
      })
    ]);
    // A project (or member) switch can outrun this read. Keep the branch list
    // attached to what it came from instead of flashing another repository's.
    if (member !== activeMemberIn(project())) {
      return;
    }

    branches = next;
    remoteBranches = onlyRemote;
    currentBranch = heads[member] ?? "";
    hasRemote = remote !== null;
  }

  return {
    get branches() {
      return branches;
    },
    get remoteBranches() {
      return remoteBranches;
    },
    get currentBranch() {
      return currentBranch;
    },
    get hasRemote() {
      return hasRemote;
    },
    /** Re-read the branches of the repository the top bar points at. */
    load,
    /** Forget the branches - the window no longer holds a project. */
    clear
  };
}
