// Context-window tracking per session (SoC: shared state in lib/stores). Powers
// auto-handoff: when an agent nears its context limit we hand off to a fresh one.
//
// Three signals, strongest first:
//   1. The agent's own session log, read off disk by `session_context.rs` — the
//      `usage` block of its newest turn against its model's real window. Exact,
//      independent of anything the terminal paints, and available from the first
//      turn onward, so it outranks everything below whenever it answers.
//   2. Parse the agent CLI's own context indicator out of the PTY stream (exact,
//      but coupled to that CLI's output — heuristic, tune against real output).
//      Claude Code only prints one in the last ~3% of the window, so for most of
//      a session this is silent; it stays as the answer for agents with no log.
//   3. Estimate from the bytes seen through the PTY (rough, agent-agnostic). A
//      fullscreen agent repaints its whole frame on every spinner tick, so this
//      over-counts badly and must never end a session; it feeds only the soft
//      tab gauge (`contextPercentage`), never auto-handoff / resume / retry.

import { SvelteMap } from "svelte/reactivity";

/** Rough characters-per-token ratio for the PTY-estimate fallback. */
const CHARACTERS_PER_TOKEN = 4;
/** Assumed context window (tokens) when only the estimate is available. */
const DEFAULT_CONTEXT_LIMIT = 200_000;

interface ContextSignal {
  /** Tokens the agent's own session log says the conversation occupies — the
   *  authoritative reading, written by the agent itself every turn. */
  loggedTokens: number | null;
  /** Percent of context used, parsed from the agent's own output (0..100). */
  parsedPercentage: number | null;
  /** The window size the agent announced ("Opus 4.8 (1M context)"), tokens. */
  windowTokens: number | null;
  /** Running maximum of the agent's own "N tokens" consumed counter. A max,
   *  not the latest: the screen also carries small per-turn counters ("↓ 83
   *  tokens"), and the session total only grows until the session cycles. */
  reportedTokens: number;
  /** Cumulative PTY characters seen — the estimate fallback. */
  characters: number;
}

const signals = new SvelteMap<string, ContextSignal>();

/** Scale a token count like "123", "45k", "1m" to an absolute number. */
function scaleTokens(numberText: string, suffix: string | undefined): number | null {
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
// anchored by the hint that follows it — "342.4K (68%)  ctrl+p commands".
// The one indicator that never splits: the status sidebar's "Context …
// tokens … % used" spans separate terminal rows with main-pane text
// interleaved between them, so a joined-rows parse misses it live.
const FOOTER_USED_RE = /[\d,.]+\s*(?:k|m)?\s*\((\d{1,3})\s*%\)\s*ctrl\+p/;
// OpenCode's status sidebar, when its pieces land contiguously (a narrow
// pane, a copy-paste): "Context 14,479 tokens 3% used".
const SIDEBAR_USED_RE = /context\s+[\d,.]+\s*(?:k|m)?\s*tokens\s+(\d{1,3})\s*%\s*used/;
// Claude Code's low-context warning: "Context low (12% remaining) · Run
// /compact to compact & continue". Its own arm ahead of the loose USED_RE
// below, which would otherwise read the *remaining* 12 as 12% USED — an 88%
// full session reported as nearly empty. The anchor stops at the percent so a
// clipped line still parses: that warning shares one right-aligned,
// overflow-hidden row with the auto-updater, and "Update available! Run: winget
// upgrade Anthropic.ClaudeCode" is long enough to cut the tail off it.
const CONTEXT_LOW_RE = /context\s+low\s*\(\s*(\d{1,3})\s*%/;
// Claude Code's everyday indicator, and the one form carrying no "left" or
// "remaining" for REMAINING_RE to anchor on — the auto-compact phrase is the
// anchor instead. Counts DOWN to compaction, so it reads as remaining.
const UNTIL_COMPACT_RE = /(\d{1,3})\s*%\s*until\s+auto-?\s*compact/;
// The bare "left … N%" arm needs a context/window anchor: an agent transcript
// can carry arbitrary pasted content (CSS with `left:` and percentages dumped
// by a tool call), and an unanchored match there read as "context nearly full"
// on a session that was at 3% — a false handoff.
const REMAINING_RE = /(\d{1,3})\s*%\s*(?:context\s*)?(?:left|remaining)|(?:context|window)[^%\d]{0,24}(?:left|remaining)[^%\d]{0,24}(\d{1,3})\s*%/;
const USED_RE = /(\d{1,3})\s*%\s*context|context[^%\d]{0,24}(\d{1,3})\s*%/;
const RATIO_RE = /([\d,]+)\s*(k|m)?\s*\/\s*([\d,]+)\s*(k|m)?\s*tokens/;

// The window size the agent announces in its banner — "Opus 4.8 (1M context)",
// "(200K context)". The one anchor that turns a raw consumed-tokens counter
// into a percent, since agents only print their own % indicator near the limit
// while a low threshold needs the fill long before that.
const WINDOW_RE = /\((\d+(?:\.\d+)?)\s*(k|m)\s*context\)/;
// Every standalone "N tokens" counter on screen (the transcript total, the
// per-turn spinner count). Used/limit ratios are stripped first so their limit
// side is never mistaken for consumption.
const TOKENS_RE = /(\d[\d,]*(?:\.\d+)?)\s*(k|m)?\s*tokens\b/g;
const RATIO_STRIP_RE = /[\d,.]+\s*(?:k|m)?\s*\/\s*[\d,.]+\s*(?:k|m)?\s*tokens\b/g;

/** Best-effort parse of a context "percent used" from a chunk of agent output. */
function parseUsedPercentage(text: string): number | null {
  const lower = text.toLowerCase();

  const footerUsed = lower.match(FOOTER_USED_RE);
  if (footerUsed) {
    const percentage = Number(footerUsed[1]);
    return Number.isFinite(percentage) ? Math.min(100, percentage) : null;
  }

  const sidebarUsed = lower.match(SIDEBAR_USED_RE);
  if (sidebarUsed) {
    const percentage = Number(sidebarUsed[1]);
    return Number.isFinite(percentage) ? Math.min(100, percentage) : null;
  }

  const contextLow = lower.match(CONTEXT_LOW_RE);
  if (contextLow) {
    const percentage = Number(contextLow[1]);
    return Number.isFinite(percentage) ? Math.max(0, 100 - percentage) : null;
  }

  const untilCompact = lower.match(UNTIL_COMPACT_RE);
  if (untilCompact) {
    const percentage = Number(untilCompact[1]);
    return Number.isFinite(percentage) ? Math.max(0, 100 - percentage) : null;
  }

  const remaining = lower.match(REMAINING_RE);
  if (remaining) {
    const percentage = Number(remaining[1] ?? remaining[2]);
    return Number.isFinite(percentage) ? Math.max(0, 100 - percentage) : null;
  }

  const used = lower.match(USED_RE);
  if (used) {
    const percentage = Number(used[1] ?? used[2]);
    return Number.isFinite(percentage) ? Math.min(100, percentage) : null;
  }

  const ratio = lower.match(RATIO_RE);
  if (ratio) {
    const usedTokens = scaleTokens(ratio[1], ratio[2]);
    const limitTokens = scaleTokens(ratio[3], ratio[4]);
    if (usedTokens !== null && limitTokens !== null && limitTokens > 0) {
      return Math.min(100, (usedTokens / limitTokens) * 100);
    }
  }

  return null;
}

const EMPTY_SIGNAL: ContextSignal = {
  loggedTokens: null,
  parsedPercentage: null,
  windowTokens: null,
  reportedTokens: 0,
  characters: 0
};

/** The announced window size and the largest consumed-tokens counter in a
 *  piece of agent text ("null"/0 when absent). */
function parseTokenSignals(text: string): {
  windowTokens: number | null;
  reportedTokens: number;
} {
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
}): void {
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
}): void {
  absorb({
    id,
    text: chunk,
    characters: chunk.length
  });
}

/** Feed rendered screen text (xterm buffer rows) through the parsed signal
 *  only. A TUI's cursor-motion optimizations can split a word across the wire
 *  — Claude paints "97% contex", skips the unchanged "t" cell with a
 *  cursor-forward, then " used" — so the stream never carries the phrase the
 *  parser needs. The screen always does; this is the reliable source for the
 *  parsed percent. Never counts toward the byte estimate: these are repainted
 *  cells, not new output. */
export function observeContextScreen({ id, text }: {
  id: string;
  text: string;
}): void {
  absorb({
    id,
    text,
    characters: 0
  });
}

/** When the window banner was never seen (it paints once at spawn and can be
 *  trimmed out of a long session's replayable history), assume the LARGEST
 *  window an agent runs — deliberately under-reporting. A small window assumed
 *  large delays the tokens-derived handoff but the agent's own % indicator
 *  still fires it near the limit; a large window assumed small would cycle a
 *  1M session at a twentieth of its life. */
const FALLBACK_WINDOW_TOKENS = 1_000_000;

/** The percent the agent's own consumed-tokens counter implies, or null until a
 *  counter has been seen at all. `allowFallback` guesses the window (1M) when the
 *  agent never announced one — fine for the display gauge, but NOT for ending a
 *  session: `reportedTokens` is a sticky max of every "N tokens" ever on screen,
 *  so one tool-output count (a Gemini response's token total in an AI project)
 *  pins it high, and dividing that by a guessed window fires a false handoff on an
 *  agent, like Codex, that prints no window banner. Ending a session needs a real
 *  denominator, so the handoff path passes `allowFallback: false`. */
function tokensDerivedPercentage({ signal, allowFallback }: {
  signal: ContextSignal;
  allowFallback: boolean;
}): number | null {
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
function loggedPercentage(signal: ContextSignal): number | null {
  if (signal.loggedTokens === null || signal.windowTokens === null) {
    return null;
  }

  return Math.min(100, (signal.loggedTokens / signal.windowTokens) * 100);
}

/** The session's context usage percent (logged if known, else parsed, else
 *  estimated), or null when nothing has been observed yet. */
export function contextPercentage(id: string): number | null {
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
 *  back to the byte estimate — that estimate counts every byte a fullscreen
 *  agent repaints (spinners, elapsed-time ticks, whole-frame redraws), so it
 *  balloons far past real usage and must never end a session. Auto-handoff,
 *  usage-resume, and API-error retry all gate on this, so they act only on a fill
 *  the agent itself vouches for; a `null` reads as "room to spare" everywhere,
 *  the safe default.
 *
 *  Three agent-vouched sources, strongest first: the session log's own token
 *  accounting, the % indicator when the agent prints one, then the consumed-
 *  tokens counter against the announced window. The log is what makes a low
 *  handoff threshold workable — a screen-scraped counter can only see what the
 *  TUI chose to paint, and Claude paints no percentage at all until the last
 *  ~3% of the window. */
export function measuredContextPercentage(id: string): number | null {
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
 *  are optional — a session whose first turn isn't written yet has no usage, and
 *  an unreachable model catalog leaves the window unknown — and each is folded in
 *  only when it answers, so a later reading never erases an earlier one.
 *
 *  The window never overrides one the agent's own `(N context)` banner supplied,
 *  so a live reading still wins; the token count always overwrites, because a
 *  newer turn is strictly better than the turn before it. */
export function observeSessionLog({ id, usedTokens, windowTokens }: {
  id: string;
  usedTokens: number | null;
  windowTokens: number | null;
}): void {
  const previous = signals.get(id) ?? EMPTY_SIGNAL;
  signals.set(id, {
    ...previous,
    loggedTokens: usedTokens ?? previous.loggedTokens,
    windowTokens: previous.windowTokens ?? windowTokens
  });
}

/** Forget a session's context when it ends. */
export function dropContext(id: string): void {
  signals.delete(id);
}
