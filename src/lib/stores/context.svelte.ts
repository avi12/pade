// Context-window tracking per session (SoC: shared state in lib/stores). Powers
// auto-handoff: when an agent nears its context limit we hand off to a fresh one.
//
// Three signals, strongest first:
//   1. The agent's own session log, read off disk by `session_context.rs` - the
//      `usage` block of its newest turn against its model's real window. Exact,
//      independent of anything the terminal paints, and available from the first
//      turn onward, so it outranks everything below whenever it answers.
//   2. Parse the agent CLI's own context indicator out of the PTY stream (exact,
//      but coupled to that CLI's output - heuristic, tune against real output).
//      Claude Code only prints one in the last ~3% of the window, so for most of
//      a session this is silent; it stays as the answer for agents with no log.
//      Claude's "N% until auto-compact" is NOT one: it counts down to its own
//      compaction point, so it is kept apart as `compactionHeadroom`.
//   3. Estimate from the bytes seen through the PTY (rough, agent-agnostic). A
//      fullscreen agent repaints its whole frame on every spinner tick, so this
//      over-counts badly and must never end a session; it feeds only the soft
//      tab gauge (`contextPercentage`), never auto-handoff / resume / retry.

import { modelName } from "@/lib/model-label";
import { SvelteMap } from "svelte/reactivity";

/** Rough characters-per-token ratio for the PTY-estimate fallback. */
const CHARACTERS_PER_TOKEN = 4;
/** Assumed context window (tokens) when only the estimate is available. */
const DEFAULT_CONTEXT_LIMIT = 200_000;

interface ContextSignal {
  /** Tokens the agent's own session log says the conversation occupies - the
   *  authoritative reading, written by the agent itself every turn. */
  loggedTokens: number | null;
  /** Percent of context used, parsed from the agent's own output (0..100). */
  parsedPercentage: number | null;
  /** Percent left before the agent compacts its own conversation - Claude's
   *  "N% until auto-compact". It counts down to the compaction point, which can
   *  sit far below the window, so it is never read as a share of the window.
   *  Read off the whole screen only, so it clears once the countdown is gone. */
  compactionHeadroom: number | null;
  /** The window size the agent announced ("Opus 4.8 (1M context)"), tokens. */
  windowTokens: number | null;
  /** Running maximum of the agent's own "N tokens" consumed counter. A max,
   *  not the latest: the screen also carries small per-turn counters ("↓ 83
   *  tokens"), and the session total only grows until the session cycles. */
  reportedTokens: number;
  /** Cumulative PTY characters seen - the estimate fallback. */
  characters: number;
  /** The model the session runs (`claude-opus-5`), as the agent itself
   *  reported it (see running_model.rs). Never read off the terminal. */
  model: string | null;
  /** The reasoning effort it runs at (`high`), when the model has one. */
  effort: string | null;
  /** Claude's Ultracode (multi-agent orchestration) is on for the session. */
  ultracode: boolean;
}

const signals = new SvelteMap<string, ContextSignal>();

/** Scale a token count like "123", "45k", "1m" to an absolute number. */
function scaleTokens(numberText: string, suffix: string | undefined) {
  const base = Number(numberText.replaceAll(",", ""));
  if (!Number.isFinite(base)) {
    return null;
  }

  if (suffix === "k") {
    return base * 1_000;
  }

  if (suffix === "m") {
    return base * 1_000_000;
  }

  return base;
}

// Match the common shapes an agent CLI prints and normalize to "percent USED".
// OpenCode's footer prints its own percent-of-window on one row, uniquely
// anchored by the hint that follows it - "342.4K (68%)  ctrl+p commands".
// The one indicator that never splits: the status sidebar's "Context …
// tokens … % used" spans separate terminal rows with main-pane text
// interleaved between them, so a joined-rows parse misses it live.
const FOOTER_USED_RE = /[\d,.]+\s*(?:k|m)?\s*\((\d{1,3})\s*%\)\s*ctrl\+p/;
// OpenCode's status sidebar, when its pieces land contiguously (a narrow
// pane, a copy-paste): "Context 14,479 tokens 3% used".
const SIDEBAR_USED_RE = /context\s+[\d,.]+\s*(?:k|m)?\s*tokens\s+(\d{1,3})\s*%\s*used/;
// Claude Code's low-context warning: "Context low (12% remaining) · Run
// /compact to compact & continue". Its own arm ahead of the loose USED_RE
// below, which would otherwise read the *remaining* 12 as 12% USED - an 88%
// full session reported as nearly empty. The anchor stops at the percent so a
// clipped line still parses: that warning shares one right-aligned,
// overflow-hidden row with the auto-updater, and "Update available! Run: winget
// upgrade Anthropic.ClaudeCode" is long enough to cut the tail off it.
const CONTEXT_LOW_RE = /context\s+low\s*\(\s*(\d{1,3})\s*%/;
// Claude Code's countdown to its own compaction, in both of its wordings: "23%
// until auto-compact" and "Context left until auto-compact: 23%". It measures
// the room left before the compaction point, not the window - a 1M session can
// compact at 300k - so it feeds `compactionHeadroom` and is stripped before
// the percent-used parse, which would otherwise read "2% until" as 98% full.
const COMPACTION_HEADROOM_RE = /(\d{1,3})\s*%\s*until\s+auto-?\s*compact|left\s+until\s+auto-?\s*compact\s*:?\s*(\d{1,3})\s*%/;
const COMPACTION_HEADROOM_STRIP_RE = new RegExp(COMPACTION_HEADROOM_RE.source, "g");
// The bare "left … N%" arm needs a context/window anchor: an agent transcript
// can carry arbitrary pasted content (CSS with `left:` and percentages dumped
// by a tool call), and an unanchored match there read as "context nearly full"
// on a session that was at 3% - a false handoff.
const REMAINING_RE = /(\d{1,3})\s*%\s*(?:context\s*)?(?:left|remaining)|(?:context|window)[^%\d]{0,24}(?:left|remaining)[^%\d]{0,24}(\d{1,3})\s*%/;
const USED_RE = /(\d{1,3})\s*%\s*context|context[^%\d]{0,24}(\d{1,3})\s*%/;
const RATIO_RE = /([\d,]+)\s*(k|m)?\s*\/\s*([\d,]+)\s*(k|m)?\s*tokens/;

// The window size the agent announces in its banner - "Opus 4.8 (1M context)",
// "(200K context)". The one anchor that turns a raw consumed-tokens counter
// into a percent, since agents only print their own % indicator near the limit
// while a low threshold needs the fill long before that.
const WINDOW_RE = /\((\d+(?:\.\d+)?)\s*(k|m)\s*context\)/;
// Every standalone "N tokens" counter on screen (the transcript total, the
// per-turn spinner count). Used/limit ratios are stripped first so their limit
// side is never mistaken for consumption.
const TOKENS_RE = /(\d[\d,]*(?:\.\d+)?)\s*(k|m)?\s*tokens\b/g;
const RATIO_STRIP_RE = /[\d,.]+\s*(?:k|m)?\s*\/\s*[\d,.]+\s*(?:k|m)?\s*tokens\b/g;

// The single-percent indicators, strongest first, and whether each counts what
// is used or what is left.
const PERCENT_INDICATORS = [
  {
    pattern: FOOTER_USED_RE,
    isRemaining: false
  },
  {
    pattern: SIDEBAR_USED_RE,
    isRemaining: false
  },
  {
    pattern: CONTEXT_LOW_RE,
    isRemaining: true
  },
  {
    pattern: REMAINING_RE,
    isRemaining: true
  },
  {
    pattern: USED_RE,
    isRemaining: false
  }
] as const;

/** Best-effort parse of a context "percent used" from a chunk of agent output. */
function parseUsedPercentage(text: string) {
  const lower = text.toLowerCase().replaceAll(COMPACTION_HEADROOM_STRIP_RE, "");

  for (const { pattern, isRemaining } of PERCENT_INDICATORS) {
    const indicator = lower.match(pattern);
    if (!indicator) {
      continue;
    }

    const percentage = Number(indicator[1] ?? indicator[2]);
    if (!Number.isFinite(percentage)) {
      return null;
    }

    return isRemaining ? Math.max(0, 100 - percentage) : Math.min(100, percentage);
  }

  return parseRatioPercentage(lower);
}

/** The percent a "used / limit tokens" ratio implies, or null without one. */
function parseRatioPercentage(lower: string) {
  const ratio = lower.match(RATIO_RE);
  if (!ratio) {
    return null;
  }

  const usedTokens = scaleTokens(ratio[1], ratio[2]);
  const limitTokens = scaleTokens(ratio[3], ratio[4]);
  if (usedTokens === null || limitTokens === null || limitTokens <= 0) {
    return null;
  }

  return Math.min(100, (usedTokens / limitTokens) * 100);
}

const EMPTY_SIGNAL: ContextSignal = {
  loggedTokens: null,
  parsedPercentage: null,
  compactionHeadroom: null,
  windowTokens: null,
  reportedTokens: 0,
  characters: 0,
  model: null,
  effort: null,
  ultracode: false
};

/** The announced window size and the largest consumed-tokens counter in a
 *  piece of agent text ("null"/0 when absent). */
function parseTokenSignals(text: string) {
  const lower = text.toLowerCase();

  let windowTokens: number | null = null;
  const window = lower.match(WINDOW_RE);
  if (window) {
    windowTokens = scaleTokens(window[1], window[2]);
  }

  let reportedTokens = 0;
  for (const counter of lower.replaceAll(RATIO_STRIP_RE, "").matchAll(TOKENS_RE)) {
    const tokens = scaleTokens(counter[1], counter[2]);
    if (tokens !== null && tokens > reportedTokens) {
      reportedTokens = tokens;
    }
  }

  return {
    windowTokens,
    reportedTokens
  };
}

/** Fold one observation's parse results into a session's stored signal. */
function absorb({ id, text, characters }: {
  id: string;
  text: string;
  characters: number;
}) {
  const previous = signals.get(id) ?? EMPTY_SIGNAL;
  const parsed = parseUsedPercentage(text);
  const tokens = parseTokenSignals(text);
  signals.set(id, {
    ...previous,
    parsedPercentage: parsed ?? previous.parsedPercentage,
    windowTokens: tokens.windowTokens ?? previous.windowTokens,
    reportedTokens: Math.max(tokens.reportedTokens, previous.reportedTokens),
    characters: previous.characters + characters
  });
}

/** Feed a chunk of a session's PTY output through the context signals. */
export function observeContext({ id, chunk }: {
  id: string;
  chunk: string;
}) {
  absorb({
    id,
    text: chunk,
    characters: chunk.length
  });
}

/** Feed rendered screen text (xterm buffer rows) through the parsed signal
 *  only. A TUI's cursor-motion optimizations can split a word across the wire
 *  - Claude paints "97% contex", skips the unchanged "t" cell with a
 *  cursor-forward, then " used" - so the stream never carries the phrase the
 *  parser needs. The screen always does; this is the reliable source for the
 *  parsed percent. Never counts toward the byte estimate: these are repainted
 *  cells, not new output. */
export function observeContextScreen({ id, text }: {
  id: string;
  text: string;
}) {
  absorb({
    id,
    text,
    characters: 0
  });

  const previous = signals.get(id) ?? EMPTY_SIGNAL;
  const compactionHeadroom = parseCompactionHeadroom(text);
  if (compactionHeadroom !== previous.compactionHeadroom) {
    signals.set(id, {
      ...previous,
      compactionHeadroom
    });
  }
}

/** The percent left until the agent compacts its conversation, or null when
 *  no countdown is on screen. */
function parseCompactionHeadroom(text: string) {
  const countdown = text.toLowerCase().match(COMPACTION_HEADROOM_RE);
  if (!countdown) {
    return null;
  }

  const percentage = Number(countdown[1] ?? countdown[2]);
  return Number.isFinite(percentage) ? percentage : null;
}

/** How much room the agent says is left before it compacts its own
 *  conversation (percent), or null when it shows no countdown. */
export function compactionHeadroom(id: string) {
  return signals.get(id)?.compactionHeadroom ?? null;
}

/** When the window banner was never seen (it paints once at spawn and can be
 *  trimmed out of a long session's replayable history), assume the LARGEST
 *  window an agent runs - deliberately under-reporting. A small window assumed
 *  large delays the tokens-derived handoff but the agent's own % indicator
 *  still fires it near the limit; a large window assumed small would cycle a
 *  1M session at a twentieth of its life. */
const FALLBACK_WINDOW_TOKENS = 1_000_000;

/** The percent the agent's own consumed-tokens counter implies, or null until a
 *  counter has been seen at all. `allowFallback` guesses the window (1M) when the
 *  agent never announced one - fine for the display gauge, but NOT for ending a
 *  session: `reportedTokens` is a sticky max of every "N tokens" ever on screen,
 *  so one tool-output count (a Gemini response's token total in an AI project)
 *  pins it high, and dividing that by a guessed window fires a false handoff on an
 *  agent, like Codex, that prints no window banner. Ending a session needs a real
 *  denominator, so the handoff path passes `allowFallback: false`. */
function tokensDerivedPercentage({ signal, allowFallback }: {
  signal: ContextSignal;
  allowFallback: boolean;
}) {
  if (signal.reportedTokens === 0) {
    return null;
  }

  const window = signal.windowTokens ?? (allowFallback ? FALLBACK_WINDOW_TOKENS : null);
  if (window === null) {
    return null;
  }

  return Math.min(100, (signal.reportedTokens / window) * 100);
}

/** The percent the agent's own session log implies, or null until both halves of
 *  the fraction are known. The strongest signal there is: the agent wrote both
 *  numbers itself, so nothing here depends on what its TUI happened to paint. */
function loggedPercentage(signal: ContextSignal) {
  if (signal.loggedTokens === null || signal.windowTokens === null) {
    return null;
  }

  return Math.min(100, (signal.loggedTokens / signal.windowTokens) * 100);
}

/** The session's context usage percent (logged if known, else parsed, else
 *  estimated), or null when nothing has been observed yet. */
export function contextPercentage(id: string) {
  const signal = signals.get(id);
  if (!signal) {
    return null;
  }

  const logged = loggedPercentage(signal);
  if (logged !== null) {
    return logged;
  }

  if (signal.parsedPercentage !== null) {
    return signal.parsedPercentage;
  }

  const derived = tokensDerivedPercentage({
    signal,
    allowFallback: true
  });
  if (derived !== null) {
    return derived;
  }

  if (signal.characters === 0) {
    return null;
  }

  const tokens = signal.characters / CHARACTERS_PER_TOKEN;
  return Math.min(100, (tokens / DEFAULT_CONTEXT_LIMIT) * 100);
}

/** The session's context fill as the AGENT ITSELF accounts for it, or null when
 *  it has vouched for nothing yet. Unlike `contextPercentage` this never falls
 *  back to the byte estimate - that estimate counts every byte a fullscreen
 *  agent repaints (spinners, elapsed-time ticks, whole-frame redraws), so it
 *  balloons far past real usage and must never end a session. Auto-handoff,
 *  usage-resume, and API-error retry all gate on this, so they act only on a fill
 *  the agent itself vouches for; a `null` reads as "room to spare" everywhere,
 *  the safe default.
 *
 *  Three agent-vouched sources, strongest first: the session log's own token
 *  accounting, the % indicator when the agent prints one, then the consumed-
 *  tokens counter against the announced window. The log is what makes a low
 *  handoff threshold workable - a screen-scraped counter can only see what the
 *  TUI chose to paint, and Claude paints no percentage at all until the last
 *  ~3% of the window. */
export function measuredContextPercentage(id: string) {
  const signal = signals.get(id);
  if (!signal) {
    return null;
  }

  return loggedPercentage(signal) ?? signal.parsedPercentage ?? tokensDerivedPercentage({
    signal,
    allowFallback: false
  });
}

/** Record what the agent's own session log says about this session: the tokens
 *  its newest turn occupies, and the window its model advertises. Both halves
 *  are optional - a session whose first turn isn't written yet has no usage, and
 *  an unreachable model catalog leaves the window unknown - and each is folded in
 *  only when it answers, so a later reading never erases an earlier one.
 *
 *  The window never overrides one the agent's own `(N context)` banner supplied,
 *  so a live reading still wins; the token count always overwrites, because a
 *  newer turn is strictly better than the turn before it. */
export function observeSessionLog({ id, usedTokens, windowTokens }: {
  id: string;
  usedTokens: number | null;
  windowTokens: number | null;
}) {
  const previous = signals.get(id) ?? EMPTY_SIGNAL;
  signals.set(id, {
    ...previous,
    loggedTokens: usedTokens ?? previous.loggedTokens,
    windowTokens: previous.windowTokens ?? windowTokens
  });
}

/** Record the model and effort the agent itself reported for this session.
 *  Each reading replaces the last whole - the agent reports both together, and
 *  a model switch can drop the effort (a model without one). */
export function observeRunningModel({ id, model, effort, ultracode }: {
  id: string;
  model: string;
  effort: string | null;
  ultracode: boolean;
}) {
  const previous = signals.get(id) ?? EMPTY_SIGNAL;
  signals.set(id, {
    ...previous,
    model,
    effort,
    ultracode
  });
}

/** The session's running model - its id, and its name as a person reads it -
 *  with its effort, or null until the agent has reported one. */
export function sessionModel(id: string) {
  const signal = signals.get(id);
  if (signal === undefined || signal.model === null) {
    return null;
  }

  return {
    id: signal.model,
    name: modelName(signal.model),
    effort: signal.effort,
    ultracode: signal.ultracode
  };
}

/** Forget a session's context when it ends. */
export function dropContext(id: string) {
  signals.delete(id);
}
