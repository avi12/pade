// The agents dashboard's wire between windows, and the pure rules that read it.
//
// Every project window owns its own sessions, so the dashboard can't ask the
// backend what they are doing - each window publishes a snapshot of its agents
// on one BroadcastChannel (the windows share an origin), and the dashboard
// window listens. The dashboard announces itself so windows know someone is
// watching (and only then spend a model call summarizing their agents); a row
// click asks the owning window to bring that session's tab forward.

import { SessionStatus } from "@/lib/types";
import { formatAge, formatQuantity } from "@/lib/util/format";
import { z } from "zod";

export const BOARD_CHANNEL = "pade.agents-board";

/** How often the open dashboard re-announces itself. */
export const DASHBOARD_HEARTBEAT_MS = 60_000;
/** A window counts the dashboard as open this long after its last announcement. */
const DASHBOARD_PRESENCE_MS = DASHBOARD_HEARTBEAT_MS * 1.5;

/** No summary is asked for sooner than this after the last one. */
const MINIMUM_SUMMARY_GAP_MS = 30_000;
/** A working agent's summary is refreshed this often. */
const WORKING_SUMMARY_REFRESH_MS = 2 * 60_000;
/** How many changed files a window's card names. */
const RECENT_FILE_COUNT = 5;

const BoardAgent = z.object({
  id: z.string(),
  /** The agent's id, for its icon. */
  agentId: z.string(),
  agentLabel: z.string(),
  /** The tab's name - its AI or hand-given label, else the agent's. */
  name: z.string(),
  /** The project the session belongs to - a window holds several. */
  project: z.string(),
  /** The conversation it runs, so another window's rail never also offers it
   *  as one to resume. */
  conversationId: z.string().optional(),
  status: SessionStatus,
  isAwaitingChoice: z.boolean(),
  /** The running model and effort as the tab's tag says it. */
  model: z.string().nullable(),
  summary: z.string().nullable(),
  summarizedAt: z.number().nullable(),
  /** When the status last changed. */
  statusSince: z.number()
});
export type BoardAgent = z.infer<typeof BoardAgent>;

const BoardWindow = z.object({
  label: z.string(),
  project: z.string(),
  projectLabel: z.string(),
  agents: z.array(BoardAgent),
  /** Distinct changed files in the project, newest first. */
  changedFiles: z.array(z.string()),
  changedFileCount: z.number(),
  lastChangeAt: z.number().nullable()
});
export type BoardWindow = z.infer<typeof BoardWindow>;

export const BoardMessageKind = z.enum(["dashboardOpen", "dashboardClosed", "rollCall", "window", "windowGone", "focus"]);
export type BoardMessageKind = z.infer<typeof BoardMessageKind>;

const BoardMessage = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal(BoardMessageKind.enum.dashboardOpen) }),
  // A project window that just opened asks the others what they run, for its
  // left rail - without counting as a watching dashboard (no summaries paid).
  z.object({ kind: z.literal(BoardMessageKind.enum.rollCall) }),
  z.object({ kind: z.literal(BoardMessageKind.enum.dashboardClosed) }),
  z.object({
    kind: z.literal(BoardMessageKind.enum.window),
    window: BoardWindow
  }),
  z.object({
    kind: z.literal(BoardMessageKind.enum.windowGone),
    label: z.string()
  }),
  z.object({
    kind: z.literal(BoardMessageKind.enum.focus),
    label: z.string(),
    sessionId: z.string()
  })
]);
export type BoardMessage = z.infer<typeof BoardMessage>;

/** Another window's message is a trust boundary like any other. */
export function parseBoardMessage(raw: unknown) {
  return BoardMessage.safeParse(raw);
}

/** The board before any window has reported. */
export const NO_BOARD_WINDOWS: ReadonlyMap<string, BoardWindow> = new Map();

/** The windows known to the board after `message`: a snapshot replaces its
 *  window's last one, a goodbye drops it, anything else changes nothing. */
export function windowsAfterMessage({ windows, message }: {
  windows: ReadonlyMap<string, BoardWindow>;
  message: BoardMessage;
}) {
  if (message.kind === BoardMessageKind.enum.window) {
    return new Map(windows).set(message.window.label, message.window);
  }

  if (message.kind === BoardMessageKind.enum.windowGone && windows.has(message.label)) {
    const next = new Map(windows);
    next.delete(message.label);
    return next;
  }

  return windows;
}

/** The windows still open - one that closed without saying goodbye (a crash,
 *  a kill) leaves the board once `window_list` no longer lists it. */
export function windowsStillOpen({ windows, openLabels }: {
  windows: ReadonlyMap<string, BoardWindow>;
  openLabels: readonly string[];
}) {
  const isEveryWindowOpen = [...windows.keys()].every(label => openLabels.includes(label));
  if (isEveryWindowOpen) {
    return windows;
  }

  return new Map([...windows].filter(([label]) => openLabels.includes(label)));
}

/** A session another PADE window runs, with the window it runs in. */
export type ElsewhereSession = BoardAgent & { windowLabel: string };

/** Every session the other windows run, for this window's left rail. */
export function elsewhereSessions({ windows, ownLabel }: {
  windows: ReadonlyMap<string, BoardWindow>;
  ownLabel: string;
}) {
  const sessions: ElsewhereSession[] = [];
  for (const [label, board] of windows) {
    if (label === ownLabel) {
      continue;
    }

    for (const agent of board.agents) {
      sessions.push({
        ...agent,
        windowLabel: label
      });
    }
  }

  return sessions;
}

/** What an agent amounts to at a glance, most urgent first. */
export const AgentState = {
  Waiting: "waiting",
  Working: "working",
  Starting: "starting",
  Ready: "ready",
  Exited: "exited"
} as const;
export type AgentState = (typeof AgentState)[keyof typeof AgentState];

const AGENT_STATE_ORDER: readonly AgentState[] = [
  AgentState.Waiting,
  AgentState.Working,
  AgentState.Starting,
  AgentState.Ready,
  AgentState.Exited
];

/** What the dashboard calls each state. */
export const AGENT_STATE_TEXT: Record<AgentState, string> = {
  [AgentState.Waiting]: "Needs you",
  [AgentState.Working]: "Working",
  [AgentState.Starting]: "Starting",
  [AgentState.Ready]: "Idle",
  [AgentState.Exited]: "Exited"
};

const STATE_OF_STATUS: Record<SessionStatus, AgentState> = {
  [SessionStatus.enum.starting]: AgentState.Starting,
  [SessionStatus.enum.working]: AgentState.Working,
  [SessionStatus.enum.ready]: AgentState.Ready,
  [SessionStatus.enum.exited]: AgentState.Exited
};

/** An agent waiting on the user's choice outranks whatever its status says. */
export function agentState(agent: Pick<BoardAgent, "status" | "isAwaitingChoice">) {
  if (agent.isAwaitingChoice) {
    return AgentState.Waiting;
  }

  return STATE_OF_STATUS[agent.status];
}

/** How the dashboard orders its project cards. */
export const BoardSort = z.enum(["windows", "name", "attention", "activity"]);
export type BoardSort = z.infer<typeof BoardSort>;

/** The sort control's options, the stable ones first. Window order is the
 *  default: a card that moved every time its agent started or stopped could
 *  not be found again at a glance, so the live orders are the user's to pick. */
export const BOARD_SORTS = [
  {
    value: BoardSort.enum.windows,
    label: "Window order"
  },
  {
    value: BoardSort.enum.name,
    label: "Name"
  },
  {
    value: BoardSort.enum.attention,
    label: "Needs you first"
  },
  {
    value: BoardSort.enum.activity,
    label: "Recent activity"
  }
] as const;

function stateRank(state: AgentState) {
  return AGENT_STATE_ORDER.indexOf(state);
}

/** A window's most urgent agent state - Exited for a window with none. */
function windowState(window: Pick<BoardWindow, "agents">) {
  let urgent: AgentState = AgentState.Exited;
  for (const agent of window.agents) {
    const state = agentState(agent);
    if (stateRank(state) < stateRank(urgent)) {
      urgent = state;
    }
  }

  return urgent;
}

/** When anything last happened in a window: a file changed or an agent's
 *  state moved. */
function lastActivity(window: BoardWindow) {
  let latest = window.lastChangeAt ?? 0;
  for (const agent of window.agents) {
    latest = Math.max(latest, agent.statusSince);
  }

  return latest;
}

/** Each sort as a comparison; every one falls back to the project's name. */
function sortKeyComparison({ sort, order }: {
  sort: BoardSort;
  order: readonly string[];
}) {
  function position(window: BoardWindow) {
    const index = order.indexOf(window.label);
    return index === -1 ? order.length : index;
  }

  const comparisons: Record<BoardSort, (left: BoardWindow, right: BoardWindow) => number> = {
    [BoardSort.enum.windows]: (left, right) => position(left) - position(right),
    [BoardSort.enum.name]: () => 0,
    [BoardSort.enum.attention]: (left, right) => stateRank(windowState(left)) - stateRank(windowState(right)),
    [BoardSort.enum.activity]: (left, right) => lastActivity(right) - lastActivity(left)
  };
  return comparisons[sort];
}

/** The windows in the order the dashboard shows them. `order` is the user's own
 *  "Open windows" order (window labels as `window_list` gives them - the one
 *  order the switcher and the Ctrl+Alt+[ / ] cycle read too); a window not in it
 *  yet sorts after. */
export function orderedWindows({ windows, order, sort }: {
  windows: Iterable<BoardWindow>;
  order: readonly string[];
  sort: BoardSort;
}) {
  const compare = sortKeyComparison({
    sort,
    order
  });
  return [...windows].sort((left, right) =>
    compare(left, right) || left.projectLabel.localeCompare(right.projectLabel));
}

/** How many agents are in each state across every window - the states that
 *  have any, most urgent first. */
export function boardTally(windows: Iterable<BoardWindow>) {
  const states: AgentState[] = [];
  for (const window of windows) {
    for (const agent of window.agents) {
      states.push(agentState(agent));
    }
  }

  return AGENT_STATE_ORDER
    .map(state => ({
      state,
      count: states.filter(counted => counted === state).length
    }))
    .filter(entry => entry.count > 0);
}

/** A project card's one-line facts: its agents, the files they changed, and
 *  how long ago the last change landed. */
export function windowFacts({ board, now }: {
  board: BoardWindow;
  now: number;
}) {
  const facts = [
    formatQuantity({
      count: board.agents.length,
      noun: "agent"
    }),
    `${formatQuantity({
      count: board.changedFileCount,
      noun: "file"
    })} changed`
  ];
  if (board.lastChangeAt !== null) {
    facts.push(
      `last ${formatAge({
        stamp: board.lastChangeAt,
        now
      })} ago`
    );
  }

  return facts.join(" · ");
}

/** The distinct paths among change events (newest first), the latest few kept. */
export function changedFiles(events: readonly { path: string }[]) {
  const paths = [...new Set(events.map(event => event.path))];
  return {
    recent: paths.slice(0, RECENT_FILE_COUNT),
    count: paths.length
  };
}

/** Whether the dashboard announced itself recently enough to count as open. */
export function isDashboardOpen({ seenAt, now }: {
  seenAt: number | null;
  now: number;
}) {
  return seenAt !== null && now - seenAt < DASHBOARD_PRESENCE_MS;
}

/** Whether an agent's summary should be refreshed now: never more often than
 *  MINIMUM_SUMMARY_GAP_MS; a first one once it works or waits, a fresh one
 *  once it stops after a stretch of work, and a periodic one while it works. */
export function isSummaryDue({ status, statusSince, summarizedAt, now }: {
  status: SessionStatus;
  statusSince: number;
  summarizedAt: number | null;
  now: number;
}) {
  const isSummarizable = status === SessionStatus.enum.working || status === SessionStatus.enum.ready;
  if (!isSummarizable) {
    return false;
  }

  if (summarizedAt === null) {
    return true;
  }

  const sinceSummary = now - summarizedAt;
  if (sinceSummary < MINIMUM_SUMMARY_GAP_MS) {
    return false;
  }

  if (status === SessionStatus.enum.ready) {
    return statusSince > summarizedAt;
  }

  return sinceSummary >= WORKING_SUMMARY_REFRESH_MS;
}
