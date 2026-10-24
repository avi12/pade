<script lang="ts">
  import IdeMenu from "@/lib/ide/IdeMenu.svelte";
  import { addRoot } from "@/lib/settings/settings-requests";
  import { settings } from "@/lib/settings/settings.svelte";
  import type { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
  import SidePanelSwitch from "@/lib/shell/screen/SidePanelSwitch.svelte";
  import { Side } from "@/lib/shell/side-panels";
  import { activeMemberIn, refreshMemberBranches } from "@/lib/stores/workspace/workspaceMembers.svelte";
  import type { AgentSession } from "@/lib/types";
  import UsageMeter from "@/lib/usage/UsageMeter.svelte";
  import BranchSwitcher from "@/lib/vcs/BranchSwitcher.svelte";
  import { openRepositoryTarget } from "@/lib/vcs/repository-links";
  import AppMenu from "@/lib/workspace/AppMenu.svelte";
  import MemberSwitcher from "@/lib/workspace/members/MemberSwitcher.svelte";
  import type { ComponentProps } from "svelte";

  // The top bar's chrome row: the project chip and its window menu, workspace
  // member and branch on the left, usage, editor and side-panel actions pushed
  // right. Switching projects lives in the left rail; everything that touches
  // this window's sessions goes back to the shell.
  let {
    project,
    label,
    isTemp,
    repository,
    sessions,
    activeSession,
    side = $bindable(),
    onsavetemp,
    onmerge,
    onterminaleditor
  }: {
    project: string;
    label: string;
    isTemp: boolean;
    repository: ReturnType<typeof createActiveRepository>;
    sessions: AgentSession[];
    activeSession: AgentSession | undefined;
    side: Side | null;
    onsavetemp: ComponentProps<typeof AppMenu>["onsavetemp"];
    onmerge: ComponentProps<typeof AppMenu>["onmerge"];
    onterminaleditor: ComponentProps<typeof IdeMenu>["onterminaleditor"];
  } = $props();

  // The repository the top bar points at, when it has a remote to open - both
  // the project button's and the branch pill's Ctrl-click open this one.
  const remoteRepository = $derived.by(() => {
    if (!repository.hasRemote) {
      return null;
    }

    return activeMemberIn(project);
  });
</script>

<!-- Chrome row: project menu on the left, actions pushed right. Session
     tabs live on their own full-width row below so they get the whole
     width and only spill to "+N" on genuine overflow. -->
<div class="chrome">
  <AppMenu
    {isTemp}
    {label}
    labels={settings.labels}
    onaddroot={async rootPath => await addRoot({
      path: rootPath,
      create: false
    })}
    {onmerge}
    {onsavetemp}
    path={project}
    {remoteRepository}
    roots={settings.roots}
  />
  <!-- Only rendered when the workspace has more than one part; a plain
       single-repo project keeps the top bar exactly as it was. -->
  <MemberSwitcher root={project} />
  <!-- The branch HEAD is on, and the menu that moves it: every local
       branch plus the ones only the remote has yet. Reports on the same
       member the branch list was read for. -->
  <BranchSwitcher
    branches={repository.branches}
    current={repository.currentBranch}
    cwd={activeMemberIn(project)}
    hasRemote={repository.hasRemote}
    onopengit={() => (side = Side.Vcs)}
    onopenremote={async () => {
      await openRepositoryTarget({
        project: remoteRepository ?? project,
        branch: repository.currentBranch
      });
    }}
    onrefresh={async () => {
      await Promise.all([repository.load(), refreshMemberBranches()]);
    }}
    remoteBranches={repository.remoteBranches}
  />
  <span class="chrome-spacer"></span>

  <UsageMeter {sessions} />
  <!-- Open a console editor (Neovim/Vim/Helix) in its own terminal tab,
       split beside the agent so you can watch and edit at once. GUI editors
       go through the OS (ide.open); these need a real TTY, which only a PADE
       terminal gives. -->
  <!-- The editor is resolved for the *project* (the shared editors-store
       key the Change Feed also reads - SSOT), while the launcher opens
       the active session's worktree when one is focused. -->
  <IdeMenu
    cwd={activeSession?.cwd ?? activeMemberIn(project)}
    {onterminaleditor}
    {project}
  />

  <SidePanelSwitch bind:side />
</div>

<style>
  /* First row: project menu + usage/IDE/panel actions. Its own labels
     fold to icons when it gets tight, independent of the tabs row below. */
  .chrome {
    display: flex;
    gap: clamp(8px, 1vw, 12px);
    align-items: center;
    min-inline-size: 0;
  }

  /* Pushes the action cluster to the right edge of the chrome row. */
  .chrome-spacer {
    flex: 1 1 0;
    min-inline-size: 0;
  }
</style>
