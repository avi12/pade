<script lang="ts">
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import RunnerDock from "@/lib/runners/RunnerDock.svelte";
  import type { StoredChatRegistry } from "@/lib/session/chat-registry";
  import { paneIdsAfterSplit, sessionsAfterReorder } from "@/lib/session/pane-layout";
  import SessionTabs from "@/lib/session/SessionTabs.svelte";
  import { SIDE_PANEL_MAXIMUM_FRACTION, SIDE_PANEL_MINIMUM_WIDTH } from "@/lib/settings/prefs.svelte";
  import type { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
  import { animatePaneIn, paneDropAt } from "@/lib/shell/screen/pane-dom";
  import ProjectSidebar from "@/lib/shell/screen/ProjectSidebar.svelte";
  import RecoveryNotes from "@/lib/shell/screen/RecoveryNotes.svelte";
  import ShellChrome from "@/lib/shell/screen/ShellChrome.svelte";
  import ShellToast from "@/lib/shell/screen/ShellToast.svelte";
  import TerminalPanes from "@/lib/shell/screen/TerminalPanes.svelte";
  import type { createSessionAutomation } from "@/lib/shell/sessionAutomation.svelte";
  import type { Side } from "@/lib/shell/side-panels";
  import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
  import { activeMemberIn } from "@/lib/stores/workspace/workspaceMembers.svelte";
  import SendSelectionButton from "@/lib/terminal/SendSelectionButton.svelte";
  import type { Agent, AgentSession, RecordedConversation } from "@/lib/types";
  import type { DragHint } from "@/lib/ui/drag-reorder";
  import { saveTempWorkspace } from "@/lib/workspace/save-temp-workspace";
  import SidePanel from "@/panels/SidePanel.svelte";
  import SidePanelResizer from "@/panels/SidePanelResizer.svelte";
  import type { ComponentProps } from "svelte";

  type ChromeProps = ComponentProps<typeof ShellChrome>;

  // The workspace screen: the top bar (chrome row + session tabs), the terminal
  // panes beside the side panel, the runner dock and the transient overlays. The
  // shell owns the sessions, the project and the side panel's choice; this lays
  // them out and keeps only the in-flight drag state to itself.
  let {
    windowSessions,
    automation,
    repository,
    agents,
    realAgents,
    bestAgent,
    project,
    projectLabel,
    isTemp,
    side = $bindable(),
    sidePanelWidth = $bindable(),
    elPanes = $bindable(),
    chatRegistry,
    ondelete,
    onopen,
    onrenamefolder,
    onreroot,
    onswitch,
    onselectchat,
    onresumeconversation,
    ondeletechat,
    onnewchat
  }: {
    windowSessions: WindowSessions;
    automation: ReturnType<typeof createSessionAutomation>;
    repository: ReturnType<typeof createActiveRepository>;
    agents: Agent[];
    /** The installed agents, without the shell fallback. */
    realAgents: Agent[];
    /** The agent a worktree launch falls back to. */
    bestAgent: Agent;
    project: string;
    projectLabel: string;
    isTemp: boolean;
    /** The open side panel; `null` means none is open. */
    side: Side | null;
    /** The side column's width, which SidePanelResizer moves live during a drag. */
    sidePanelWidth: number;
    /** The panes container, so a drop's pointer can be hit-tested against the panes. */
    elPanes?: HTMLElement;
    /** Every project's persisted chats, for the left rail. */
    chatRegistry: StoredChatRegistry;
    ondelete: ChromeProps["ondelete"];
    onopen: ChromeProps["onopen"];
    onrenamefolder: ChromeProps["onrenamefolder"];
    /** The saved temp workspace now lives at `path` - the window follows it. */
    onreroot: (path: string) => void;
    onswitch: ChromeProps["onswitch"];
    onselectchat: (target: {
      project: string;
      chatId: string;
    }) => void;
    onresumeconversation: (target: {
      project: string;
      conversation: RecordedConversation;
    }) => void;
    ondeletechat: (target: {
      project: string;
      chatId: string;
    }) => void;
    onnewchat: (project: string) => void;
  } = $props();

  let sessionTabsHandle = $state<{ closeSession: (session: AgentSession) => void } | null>(null);
  // Live drag state from the tab strip; drives the "drop here" overlay + halves.
  let dragHint = $state<DragHint | null>(null);
  // True while a split pane's header is dragged up over the tab strip - the mirror
  // gesture: the strip lights as a "drop → new tab" target and the drop pops the
  // pane out of the split (see TerminalPanes' `onpopout`). Reported by the
  // dragged pane's Terminal via the same engine hint the tab-split path uses.
  let isPaneDragOverTabs = $state(false);
  let isResizingSidePanel = $state(false);

  // Active agent id - used to show only its relevant config files.
  const activeAgent = $derived(windowSessions.activeSession?.agent.id ?? "");

  /** Close a session the way its tab's close does, animation included. */
  export function closeSession(session: AgentSession) {
    sessionTabsHandle?.closeSession(session);
  }
</script>

<div class="shell">
  <ProjectSidebar
    activeId={windowSessions.activeId}
    {ondeletechat}
    {onnewchat}
    onopenproject={onopen}
    {onresumeconversation}
    {onselectchat}
    {project}
    registry={chatRegistry}
    sessions={windowSessions.sessions}
  />

  <div class="workspace-column">
    <header class="topbar">
      <ShellChrome
        activeSession={windowSessions.activeSession}
        {isTemp}
        label={projectLabel}
        {ondelete}
        {onopen}
        {onrenamefolder}
        onsavetemp={async ({ name, root }) => await saveTempWorkspace({
          name,
          root,
          host: {
            project,
            sessionIds: windowSessions.sessions.map(session => session.id),
            continuing: windowSessions.activeSession ?? windowSessions.sessions[0],
            writeHandoffDoc: automation.writeHandoffDoc,
            reroot: onreroot,
            launch: windowSessions.launch,
            closeSessions: windowSessions.closeSessions
          }
        })}
        {onswitch}
        onterminaleditor={editor =>
          windowSessions.launch({
            agent: {
              id: `editor-${editor.id}`,
              label: editor.label,
              command: editor.command,
              // A terminal editor writes logical order like any other program.
              reordersBidi: false
            },
            // Inherit the active session's worktree, if any, else the project dir.
            cwd: windowSessions.activeSession?.cwd,
            // Open the working directory in the editor.
            args: ["."],
            split: true
          })}
        {project}
        {repository}
        sessions={windowSessions.sessions}
        bind:side
      />

      <SessionTabs
        bind:this={sessionTabsHandle}
        activeId={windowSessions.activeId}
        {agents}
        branches={repository.branches}
        onclose={windowSessions.close}
        ondraghint={hint => (dragHint = hint)}
        onlaunch={agent => windowSessions.launch({ agent })}
        onlaunchbranch={async branch => {
          // Spawn an agent on its own git worktree for `branch`, isolated from
        // the other sessions. Uses the active session's agent (or the first).
          const agent = windowSessions.activeSession?.agent ?? bestAgent;
          const cwd = await sendMessage(Command.vcsWorktreeAdd, {
            cwd: activeMemberIn(project),
            branch,
            create: false
          });
          windowSessions.launch({
            agent,
            cwd,
            branch
          });
        }}
        onmodelchoice={async pick => await automation.requestModel(pick)}
        onreorder={orderedIds => (windowSessions.sessions = sessionsAfterReorder({
          sessions: windowSessions.sessions,
          orderedIds
        }))}
        onselect={windowSessions.select}
        onsplit={drop => {
          // Show the dropped tab as a split pane on the side of the pane the
        // pointer landed on (repositioning it if already shown).
          const target = paneDropAt({
            elPanes,
            paneIds: windowSessions.paneIds,
            x: drop.pointerX,
            y: drop.pointerY
          });
          if (!target) {
            return;
          }

          windowSessions.paneIds = paneIdsAfterSplit({
            paneIds: windowSessions.paneIds,
            draggedId: drop.id,
            targetId: target.id,
            side: target.side
          });
          windowSessions.activeId = drop.id;
          animatePaneIn({
            elPanes,
            id: drop.id
          });
        }}
        paneIds={windowSessions.paneIds}
        popPaneActive={isPaneDragOverTabs}
        sessions={windowSessions.sessions}
      />
    </header>

    <main
      style:--side-panel-width="{sidePanelWidth}px"
      style:--side-panel-min-width="{SIDE_PANEL_MINIMUM_WIDTH}px"
      style:--side-panel-max-fraction="{SIDE_PANEL_MAXIMUM_FRACTION * 100}%"
      class="body"
      class:with-side={side !== null}
      data-resizing={isResizingSidePanel || undefined}
    >
      <TerminalPanes
        {agents}
        collapsingSplitPanes={windowSessions.collapsingSplitPanes}
        {dragHint}
        onexit={windowSessions.exited}
        onlaunch={agent => windowSessions.launch({
          agent,
          split: true
        })}
        onselect={windowSessions.select}
        sessions={windowSessions.sessions}
        bind:elPanes
        bind:paneIds={windowSessions.paneIds}
        bind:activeId={windowSessions.activeId}
        bind:isPaneDragOverTabs
      />

      {#if side !== null}
        <SidePanelResizer bind:width={sidePanelWidth} bind:isResizing={isResizingSidePanel} />
        <SidePanel agent={activeAgent} agents={realAgents} {project} {side} />
      {/if}
    </main>

    <RunnerDock activeSessionId={windowSessions.activeId} />

    <RecoveryNotes note={automation.note} pacing={automation.usagePacing} />
    <ShellToast />
    <SendSelectionButton activeId={windowSessions.activeId} />
  </div>
</div>

<style>
  /* The left rail beside the workspace column - the column keeps the old
     vertical stack (top bar, panes, dock) and shrinks around the rail. */
  .shell {
    display: flex;
    flex-direction: row;
    block-size: 100%;
  }

  .workspace-column {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-inline-size: 0;
    min-block-size: 0;
  }

  .topbar {
    display: flex;
    flex-shrink: 0;
    flex-direction: column;
    gap: 8px;
    min-inline-size: 0;
    padding-block: 8px;
    padding-inline: clamp(10px, 1.6vw, 16px);
    border-block-end: 1px solid var(--outline);
    background: var(--surface-1);
  }

  .body {
    /* The side column's resolved width - one authoritative expression the grid
       track and the drag divider both read, so they can never clamp differently. */
    --side-panel-track:
      clamp(
        var(--side-panel-min-width),
        var(--side-panel-width),
        var(--side-panel-max-fraction)
      );

    position: relative;
    display: grid;
    flex: 1;

    /* Explicit full-height row track: without it the implicit row is `auto`
       (content-sized), so the terminal shrinks with the window but never grows
       back - the resize observer only fires on the shrink. */
    grid-template-rows: 1fr;
    grid-template-columns: 1fr;
    min-block-size: 0;
    transition: grid-template-columns 250ms var(--ease);

    /* The side column follows the dragged `--side-panel-width`, clamped so it
       never falls below its minimum nor swallows more than 60% of the window
       (design: `Math.max(280, Math.min(width*0.6, …))`). The width tweens with
       the emphasized easing so a double-click snap-back to the default animates. */
    &.with-side {
      grid-template-columns: 1fr var(--side-panel-track);
      transition: grid-template-columns 250ms var(--ease);

      @media (prefers-reduced-motion: reduce) {
        transition: none;
      }
    }

    /* A live drag drives the frame directly - any transition would lag it behind
       the pointer, so it is switched off for the duration of the drag. */
    &[data-resizing] {
      transition: none;
    }
  }

  @media (width <= 720px) {
    .body.with-side {
      grid-template-rows: 1fr 40%;
      grid-template-columns: 1fr;
    }
  }
</style>
