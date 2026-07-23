// Context-window tracking per session (SoC: shared state in lib/stores). Powers
// auto-handoff: when an agent nears its context limit we hand off to a fresh one.
//
// Two signals:
//   1. Parse the agent CLI's own context indicator out of the PTY stream (exact,
//      but coupled to that CLI's output — heuristic, tune against real output).
//      This is the ONLY signal an automated decision may act on — see
//      `measuredContextPct`.
//   2. Estimate from the bytes seen through the PTY (rough, agent-agnostic). A
//      fullscreen agent repaints its whole frame on every spinner tick, so this
//      over-counts badly and must never end a session; it feeds only the soft
//      tab gauge (`contextPct`), never auto-handoff / resume / retry.

import { SvelteMap } from "svelte/reactivity";

/** Rough chars-per-token for the PTY-estimate fallback. */
const CHARS_PER_TOKEN = 4;
/** Assumed context window (tokens) when only the estimate is available. */
const DEFAULT_CONTEXT_LIMIT = 200_000;

interface ContextSignal {
  /** Percent of context used, parsed from the agent's own output (0..100). */
  parsedPct: number | null;
  /** The window size the agent announced ("Opus 4.8 (1M context)"), tokens. */
  windowTokens: number | null;
  /** Running maximum of the agent's own "N tokens" consumed counter. A max,
   *  not the latest: the screen also carries small per-turn counters ("↓ 83
   *  tokens"), and the session total only grows until the session cycles. */
  reportedTokens: number;
  /** Cumulative PTY chars seen — the estimate fallback. */
  chars: number;
}

const signals = new SvelteMap<string, ContextSignal>();

/** Scale a token count like "123", "45k", "1m" to an absolute number. */
function scaleTokens(num: string, suffix: string | undefined): number | null {
  const base = Number(num.replaceAll(",", ""));
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
// OpenCode's status sidebar prints the one exact form worth trusting above the
// loose heuristics: "Context 14,479 tokens 3% used".
const SIDEBAR_USED_RE = /context\s+[\d,.]+\s*(?:k|m)?\s*tokens\s+(\d{1,3})\s*%\s*used/;
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
function parseUsedPct(text: string): number | null {
  const lower = text.toLowerCase();

  const sidebarUsed = lower.match(SIDEBAR_USED_RE);
  if (sidebarUsed) {
    const pct = Number(sidebarUsed[1]);
    return Number.isFinite(pct) ? Math.min(100, pct) : null;
  }

  const remaining = lower.match(REMAINING_RE);
  if (remaining) {
    const pct = Number(remaining[1] ?? remaining[2]);
    return Number.isFinite(pct) ? Math.max(0, 100 - pct) : null;
  }

  const used = lower.match(USED_RE);
  if (used) {
    const pct = Number(used[1] ?? used[2]);
    return Number.isFinite(pct) ? Math.min(100, pct) : null;
  }

  const ratio = lower.match(RATIO_RE);
  if (ratio) {
    const usedTok = scaleTokens(ratio[1], ratio[2]);
    const limitTok = scaleTokens(ratio[3], ratio[4]);
    if (usedTok !== null && limitTok !== null && limitTok > 0) {
      return Math.min(100, (usedTok / limitTok) * 100);
    }
  }

  return null;
}

const EMPTY_SIGNAL: ContextSignal = {
  parsedPct: null,
  windowTokens: null,
  reportedTokens: 0,
  chars: 0
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
function absorb({ id, text, chars }: {
  id: string;
  text: string;
  chars: number;
}): void {
  const prev = signals.get(id) ?? EMPTY_SIGNAL;
  const parsed = parseUsedPct(text);
  const tokens = parseTokenSignals(text);
  signals.set(id, {
    parsedPct: parsed ?? prev.parsedPct,
    windowTokens: tokens.windowTokens ?? prev.windowTokens,
    reportedTokens: Math.max(tokens.reportedTokens, prev.reportedTokens),
    chars: prev.chars + chars
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
    chars: chunk.length
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
    chars: 0
  });
}

/** When the window banner was never seen (it paints once at spawn and can be
 *  trimmed out of a long session's replayable history), assume the LARGEST
 *  window an agent runs — deliberately under-reporting. A small window assumed
 *  large delays the tokens-derived handoff but the agent's own % indicator
 *  still fires it near the limit; a large window assumed small would cycle a
 *  1M session at a twentieth of its life. */
const FALLBACK_WINDOW_TOKENS = 1_000_000;

/** The percent the agent's own consumed-tokens counter implies, or null until
 *  a counter has been seen at all. */
function tokensDerivedPct(signal: ContextSignal): number | null {
  if (signal.reportedTokens === 0) {
    return null;
  }

  const window = signal.windowTokens || FALLBACK_WINDOW_TOKENS;
  return Math.min(100, (signal.reportedTokens / window) * 100);
}

/** The session's context usage percent (parsed if known, else estimated), or
 *  null when nothing has been observed yet. */
export function contextPct(id: string): number | null {
  const signal = signals.get(id);
  if (!signal) {
    return null;
  }

  if (signal.parsedPct !== null) {
    return signal.parsedPct;
  }

  const derived = tokensDerivedPct(signal);
  if (derived !== null) {
    return derived;
  }

  if (signal.chars === 0) {
    return null;
  }

  const tokens = signal.chars / CHARS_PER_TOKEN;
  return Math.min(100, (tokens / DEFAULT_CONTEXT_LIMIT) * 100);
}

/** The session's context fill from the agent's OWN reported indicator (the
 *  parsed signal alone), or null when it hasn't printed one yet. Unlike
 *  `contextPct` this never falls back to the byte estimate — that estimate
 *  counts every byte a fullscreen agent repaints (spinners, elapsed-time ticks,
 *  whole-frame redraws), so it balloons far past real usage and must never end a
 *  session. Auto-handoff, usage-resume, and API-error retry all gate on this, so
 *  they act only on a fill the agent itself vouches for; a `null` reads as "room
 *  to spare" everywhere, the safe default.
 *
 *  Two agent-vouched sources: the % indicator when the agent prints one, else
 *  the consumed-tokens counter against the announced window. The latter is
 *  what makes a low handoff threshold workable — the agent only prints its own
 *  % near the limit, but the tokens counter runs from the first turn. */
export function measuredContextPct(id: string): number | null {
  const signal = signals.get(id);
  if (!signal) {
    return null;
  }

  return signal.parsedPct ?? tokensDerivedPct(signal);
}

/** Forget a session's context when it ends. */
export function dropContext(id: string): void {
  signals.delete(id);
}
