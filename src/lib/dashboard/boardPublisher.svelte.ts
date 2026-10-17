// A project window's side of the agents dashboard (lib/dashboard/agent-board):
// publish a snapshot of this window's agents whenever any of it changes, answer
// a dashboard that just opened, bring a session's tab forward when the dashboard
// asks, and - only while a dashboard is open - keep each agent's AI summary
// fresh. Summaries cost a model call, so no one pays for them unwatched.

import {
  BOARD_CHANNEL,
  BoardMessageKind,
  changedFiles,
  isDashboardOpen,
  isSummaryDue,
  parseBoardMessage
} from "@/lib/dashboard/agent-board";
import type { BoardAgent, BoardMessage } from "@/lib/dashboard/agent-board";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { windowLabel } from "@/lib/ipc/platform";
import { modelLabel } from "@/lib/model/model-label";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { sessionModel } from "@/lib/stores/agent/context.svelte";
import { awaitingChoice } from "@/lib/stores/session/sessionAttention.svelte";
import { sessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { feedStore } from "@/lib/stores/workspace/feed.svelte";
import type { AgentSession, SessionStatus } from "@/lib/types";
import { onDestroy, onMount } from "svelte";
import { SvelteMap } from "svelte/reactivity";

/** How often a working agent's summary is reconsidered while the dashboard is
 *  open - the status changes that matter most re-check at once. */
const SUMMARY_CHECK_MS = 30_000;
/** A session not summarized yet. */
const NO_SUMMARY = {
  text: null,
  at: null
};

export function publishToBoard({ project, projectLabel, windowSessions }: {
  /** The window's open project; empty while none is. */
  project: () => string;
  projectLabel: () => string;
  windowSessions: WindowSessions;
}) {
  const channel = new BroadcastChannel(BOARD_CHANNEL);
  const label = windowLabel();
  const statusSince = new SvelteMap<string, {
    status: SessionStatus;
    at: number;
  }>();
  const summaries = new SvelteMap<string, {
    text: string;
    at: number;
  }>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- in-flight bookkeeping, never rendered
  const summarizing = new Set<string>();
  let dashboardSeenAt = $state<number | null>(null);
  let now = $state(Date.now());

  function post(message: BoardMessage) {
    channel.postMessage(message);
  }

  function statusSinceOf(id: string) {
    return statusSince.get(id)?.at ?? now;
  }

  function boardAgent(session: AgentSession): BoardAgent {
    const running = sessionModel(session.id);
    const summary = summaries.get(session.id) ?? NO_SUMMARY;
    return {
      id: session.id,
      agentId: session.agent.id,
      agentLabel: session.agent.label,
      name: sessionLabel(session.id) ?? session.agent.label,
      status: sessionStatus(session.id),
      isAwaitingChoice: awaitingChoice(session.id),
      model: running && modelLabel(running),
      summary: summary.text,
      summarizedAt: summary.at,
      statusSince: statusSinceOf(session.id)
    };
  }

  const snapshot = $derived.by(() => {
    const files = changedFiles(feedStore.events);
    return {
      label,
      project: project(),
      projectLabel: projectLabel(),
      agents: windowSessions.sessions.map(boardAgent),
      changedFiles: files.recent,
      changedFileCount: files.count,
      lastChangeAt: feedStore.events[0]?.ts ?? null
    };
  });

  function publish() {
    if (project() === "") {
      post({
        kind: BoardMessageKind.enum.windowGone,
        label
      });
      return;
    }

    post({
      kind: BoardMessageKind.enum.window,
      window: $state.snapshot(snapshot)
    });
  }

  // Stamp when each session's status last changed, so the dashboard can say
  // "working for 5m" and a summary can tell a fresh stop from an old one.
  $effect(() => {
    for (const session of windowSessions.sessions) {
      const status = sessionStatus(session.id);
      if (statusSince.get(session.id)?.status !== status) {
        statusSince.set(session.id, {
          status,
          at: Date.now()
        });
      }
    }
  });

  $effect(publish);

  async function summarize(session: AgentSession) {
    summarizing.add(session.id);
    try {
      const text = await sendMessage(Command.sessionSummarize, {
        id: session.id,
        agent: session.agent.command,
        cwd: session.cwd ?? project()
      });
      if (text !== null) {
        summaries.set(session.id, {
          text,
          at: Date.now()
        });
      }
    } catch (error) {
      console.warn("pade: could not summarize a session for the dashboard", error);
    } finally {
      summarizing.delete(session.id);
    }
  }

  function isSessionSummaryDue(session: AgentSession) {
    return !summarizing.has(session.id) && isSummaryDue({
      status: sessionStatus(session.id),
      statusSince: statusSinceOf(session.id),
      summarizedAt: summaries.get(session.id)?.at ?? null,
      now
    });
  }

  $effect(() => {
    const isWatched = isDashboardOpen({
      seenAt: dashboardSeenAt,
      now
    });
    if (!isWatched) {
      return;
    }

    for (const session of windowSessions.sessions.filter(isSessionSummaryDue)) {
      summarize(session);
    }
  });

  /** Bring a session's tab forward - into the active pane when it is already
   *  on screen in a split, else on its own. */
  function focusSession({ target, sessionId }: {
    target: string;
    sessionId: string;
  }) {
    if (target !== label) {
      return;
    }

    if (windowSessions.paneIds.includes(sessionId)) {
      windowSessions.activeId = sessionId;
    } else if (windowSessions.isOpen(sessionId)) {
      windowSessions.select(sessionId);
    }
  }

  function receive(message: BoardMessage) {
    if (message.kind === BoardMessageKind.enum.dashboardOpen) {
      dashboardSeenAt = Date.now();
      now = dashboardSeenAt;
      publish();
      return;
    }

    if (message.kind === BoardMessageKind.enum.dashboardClosed) {
      dashboardSeenAt = null;
      return;
    }

    if (message.kind === BoardMessageKind.enum.focus) {
      focusSession({
        target: message.label,
        sessionId: message.sessionId
      });
    }
  }

  onMount(() => {
    const clock = setInterval(() => (now = Date.now()), SUMMARY_CHECK_MS);
    channel.onmessage = e => {
      const parsed = parseBoardMessage(e.data);
      if (parsed.success) {
        receive(parsed.data);
      }
    };
    return () => clearInterval(clock);
  });

  // The page is going away (closed, reloaded): take this window off the board.
  onMount(() => {
    const leave = () => post({
      kind: BoardMessageKind.enum.windowGone,
      label
    });
    addEventListener("pagehide", leave);
    return () => removeEventListener("pagehide", leave);
  });

  onDestroy(() => channel.close());
}
