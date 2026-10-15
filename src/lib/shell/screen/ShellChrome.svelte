<script lang="ts">
  import IdeMenu from "@/lib/ide/IdeMenu.svelte";
  import { Command } from "@/lib/ipc/messaging";
  import { addRoot, sendSettingsMessage } from "@/lib/settings/settings-requests";
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
  import MemberSwitcher from "@/lib/workspace/MemberSwitcher.svelte";
  import type { ComponentProps } from "svelte";

  type AppMenuProps = ComponentProps<typeof AppMenu>;

  // The top bar's chrome row: the project menu, workspace member and branch on
  // the left, usage, design, editor and side-panel actions pushed right. The
  // pinned/recent lists persist straight into shared settings; everything that
  // touches this window's sessions goes back to the shell.
  let {
    project,
    label,
    isTemp,
    repository,
    sessions,
    activeSession,
    side = $bindable(),
    onopen,
    onswitch,
    ondelete,
    onrenamefolder,
    onsavetemp,
    onterminaleditor
  }: {
    project: string;
    label: string;
    isTemp: boolean;
    repository: ReturnType<typeof createActiveRepository>;
    sessions: AgentSession[];
    activeSession: AgentSession | undefined;
    side: Side | null;
    onopen: AppMenuProps["onopen"];
    onswitch: AppMenuProps["onswitch"];
    ondelete: AppMenuProps["ondelete"];
    onrenamefolder: AppMenuProps["onrenamefolder"];
    onsavetemp: AppMenuProps["onsavetemp"];
    onterminaleditor: ComponentProps<typeof IdeMenu>["onterminaleditor"];
  } = $props();
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
    onclearrecent={async () => {
      // Clear the recent-projects history (pins survive).
      await sendSettingsMessage(Command.workspaceClearRecent);
    }}
    {ondelete}
    {onopen}
    onremoverecent={async projectPath => {
      // Forget a project from the switcher's lists (folder untouched).
      await sendSettingsMessage(Command.workspaceRemoveRecent, { path: projectPath });
    }}
    {onrenamefolder}
    onreorderpins={async paths => {
      await sendSettingsMessage(Command.workspaceSetPinnedOrder, { paths });
    }}
    {onsavetemp}
    {onswitch}
    ontogglepin={async target => {
      // The settings request adopts the response.
      await sendSettingsMessage(Command.workspaceSetPinned, target);
    }}
    path={project}
    pinnedProjects={settings.pinnedProjects}
    recentProjects={settings.recentProjects}
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
        project,
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
  /* First row: project menu + usage/design/IDE/panel actions. Its own labels
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
