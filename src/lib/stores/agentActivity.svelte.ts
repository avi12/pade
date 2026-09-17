// Agent-reported activity: whether the agent itself says it still has work in
// flight — a running turn, or work it delegated and is waiting on (background
// agents, dynamic workflows). PADE's own session status can't tell: it reads
// output quiet as `ready`, and a session waiting on background work can sit
// quiet for many minutes. The agent's terminal title can tell.
//
// Claude Code prefixes its title with a glyph from its own busy state — "◐"/"◑"
// while a turn runs OR any background agent, remote agent, teammate or dynamic
// workflow is still going, "✳" once none is. It alternates the busy frames only
// while its terminal has focus, so an unfocused session goes quiet mid-workflow
// (what let a handoff end one), but the glyph itself is rewritten on every change
// of that state — an event, not a poll. Coupled to the CLI's observable title
// (verified against Claude Code 2.1.268), the same deliberate, documented Hyrum
// dependency as the context-percent and usage-limit parsers. An agent that
// announces no such glyph reports nothing, and every gate built on it stays open.

import { SvelteMap } from "svelte/reactivity";

/** What an agent's own terminal title says about its work. */
export const AgentActivity = {
  busy: "busy",
  idle: "idle"
} as const;
export type AgentActivity = (typeof AgentActivity)[keyof typeof AgentActivity];

/** Claude Code's title spinner frames while it has work in flight, and its glyph
 *  once it has none. Each is followed by a space and the session's title. */
const BUSY_TITLE_GLYPHS = ["◐", "◑"] as const;
const IDLE_TITLE_GLYPH = "✳";
const GLYPH_SEPARATOR = " ";

// A window-title escape: OSC 0 (icon + title) or OSC 2 (title), terminated by
// BEL or ST (ESC \). Built from strings, like lib/ansi, so no raw control byte
// sits in the source.
const TITLE_SEQUENCE_RE = new RegExp("\\u001b\\][02];([^\\u0007\\u001b]*)(?:\\u0007|\\u001b\\\\)", "g");
const SEQUENCE_START = "]";
const BELL = "";
const STRING_TERMINATOR = "\\";
/** An unterminated title longer than this is not a title — stop carrying it. */
const MAXIMUM_CARRIED_LENGTH = 1024;

/** The activity a title announces, or null when it carries no activity glyph. */
export function activityFromTitle(title: string): AgentActivity | null {
  const announcesBusy = BUSY_TITLE_GLYPHS.some(glyph => title.startsWith(`${glyph}${GLYPH_SEPARATOR}`));
  if (announcesBusy) {
    return AgentActivity.busy;
  }

  if (title.startsWith(`${IDLE_TITLE_GLYPH}${GLYPH_SEPARATOR}`)) {
    return AgentActivity.idle;
  }

  return null;
}

/** The titles a stretch of output sets, in order, and the unterminated title
 *  sequence it ends on (a PTY chunk can split one) to prepend to the next stretch. */
export function readTitles(text: string): {
  titles: string[];
  carried: string;
} {
  const titles: string[] = [];
  let consumedTo = 0;
  for (const match of text.matchAll(TITLE_SEQUENCE_RE)) {
    titles.push(match[1]);
    consumedTo = match.index + match[0].length;
  }

  const lastStart = text.lastIndexOf(SEQUENCE_START);
  const tail = lastStart === -1 ? "" : text.slice(lastStart);
  const tailTerminated = tail.includes(BELL) || tail.includes(STRING_TERMINATOR);
  const endsMidSequence = lastStart >= consumedTo
    && !tailTerminated
    && tail.length <= MAXIMUM_CARRIED_LENGTH;
  return {
    titles,
    carried: endsMidSequence ? tail : ""
  };
}

const activities = new SvelteMap<string, AgentActivity>();
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per output chunk, never rendered
const carriedSequences = new Map<string, string>();
// Pending `whenAgentSettled` resolvers, settled the moment the agent stops
// reporting busy (or the session is dropped). Event-driven — no polling.
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- resolver bookkeeping, never rendered
const settleWaiters = new Map<string, (() => void)[]>();

function settleWaitersFor(id: string): void {
  const waiters = settleWaiters.get(id);
  if (!waiters) {
    return;
  }

  settleWaiters.delete(id);
  for (const settle of waiters) {
    settle();
  }
}

/** Feed a session's PTY output (live chunks, or the history replayed on attach)
 *  through the title reader. The latest title wins; one without an activity
 *  glyph clears the report, since the agent no longer announces one. */
export function observeAgentActivity({ id, chunk }: {
  id: string;
  chunk: string;
}): void {
  const { titles, carried } = readTitles(`${carriedSequences.get(id) ?? ""}${chunk}`);
  if (carried) {
    carriedSequences.set(id, carried);
  } else {
    carriedSequences.delete(id);
  }

  const latestTitle = titles.at(-1);
  if (latestTitle === undefined) {
    return;
  }

  const activity = activityFromTitle(latestTitle);
  if (activity === null) {
    activities.delete(id);
  } else if (activities.get(id) !== activity) {
    activities.set(id, activity);
  }

  if (activity !== AgentActivity.busy) {
    settleWaitersFor(id);
  }
}

/** Whether the agent itself reports work in flight — a running turn or delegated
 *  background work (reactive). False for an agent that announces nothing. */
export function agentReportsBusy(id: string): boolean {
  return activities.get(id) === AgentActivity.busy;
}

/** Resolve once the agent no longer reports work in flight, or its session is
 *  dropped — the gate every session-ending flow waits behind, so none severs a
 *  running workflow or background agent. Resolves at once for an agent that
 *  reports nothing. */
export function whenAgentSettled(id: string): Promise<void> {
  if (!agentReportsBusy(id)) {
    return Promise.resolve();
  }

  return new Promise(resolve => {
    settleWaiters.set(id, [...(settleWaiters.get(id) ?? []), resolve]);
  });
}

/** Forget a session's reported activity when it ends; anything waiting on it settles. */
export function dropAgentActivity(id: string): void {
  activities.delete(id);
  carriedSequences.delete(id);
  settleWaitersFor(id);
}
