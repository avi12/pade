// The keyboard reach into this window's sessions: the tab shortcuts (new tab,
// launch menu, close, cycle, jump by number) and the pane shortcuts of a split.
// The chords themselves are matched in lib/session/tab-shortcuts and
// lib/session/panes/pane-shortcuts; this hands them the window's sessions to act on.

import { registerPaneShortcuts } from "@/lib/session/panes/pane-shortcuts";
import { ADD_SESSION_MENU_ID } from "@/lib/session/SessionTabs.svelte";
import { registerTabShortcuts } from "@/lib/session/tab-shortcuts";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import type { Agent, AgentSession } from "@/lib/types";
import { onMount } from "svelte";

export function registerSessionShortcuts({ windowSessions, isWorkspaceShown, fallbackAgent, closeSession }: {
  windowSessions: WindowSessions;
  /** Whether the workspace screen is up - new tabs only open from there. */
  isWorkspaceShown: () => boolean;
  /** The agent a new tab runs when no session is open to copy. */
  fallbackAgent: () => Agent | undefined;
  /** Close a session the way its tab's close does. */
  closeSession: (session: AgentSession) => void;
}) {
  // A new tab copies the last session's kind (mirrors the "+" button).
  function agentForNewTab() {
    return windowSessions.projectSessions.at(-1)?.agent ?? fallbackAgent();
  }

  // Ctrl+W / Ctrl+F4 - close the active session.
  function closeActiveTab() {
    const session = windowSessions.activeSession;
    if (session) {
      closeSession(session);
    }
  }

  // Tab shortcuts: capture-phase so they win over a focused agent terminal -
  // new tab, launch menu, close, and next/previous cycling.
  onMount(() =>
    registerTabShortcuts({
      newTab() {
        if (!isWorkspaceShown()) {
          return;
        }

        const agent = agentForNewTab();
        if (agent) {
          windowSessions.launch({ agent });
        }
      },
      // Open the launch dropdown with the first agent focused, so
      // Enter fires it and Esc light-dismisses (native popover handles both).
      launchMenu() {
        if (!isWorkspaceShown()) {
          return;
        }

        const elMenu = document.getElementById(ADD_SESSION_MENU_ID);
        if (!(elMenu instanceof HTMLElement) || elMenu.matches(":popover-open")) {
          return;
        }

        elMenu.showPopover();
        elMenu.querySelector<HTMLButtonElement>("button")?.focus();
      },
      closeTab: closeActiveTab,
      next: () => windowSessions.step(1),
      previous: () => windowSessions.step(-1),
      // Ctrl+number - jump straight to a tab of the strip by position (Ctrl+9 =
      // the last one). The matcher only ever hands back an index that exists, so
      // the guard is belt-and-suspenders; selection reuses the same make-active
      // path as a click.
      selectTab(index) {
        const session = windowSessions.projectSessions[index];
        if (session) {
          windowSessions.select(session.id);
        }
      },
      tabCount: () => windowSessions.projectSessions.length
    }));

  // Pane shortcuts, active while a tab is split into panes: Ctrl+[ / Ctrl+]
  // cycle the active pane (wrapping), Ctrl+Alt+1..9 jump to the nth, and
  // Ctrl+Alt+W closes the active pane's session - the slot then animates out via
  // `out:collapsePane`, and closing the sole pane closes the tab. Selecting a
  // pane only moves focus within the split (never collapses it, unlike a tab
  // click). Capture-phase like the tab shortcuts so they beat a focused terminal.
  onMount(() =>
    registerPaneShortcuts({
      selectPane: id => (windowSessions.activeId = id),
      closeActivePane: closeActiveTab,
      paneIds: () => windowSessions.paneIds,
      activeId: () => windowSessions.activeId
    }));
}
