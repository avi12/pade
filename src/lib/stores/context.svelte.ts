// Context-window tracking per session (SoC: shared state in lib/stores). Powers
// auto-handoff: when an agent nears its context limit we hand off to a fresh one.
//
// Two signals:
//   1. Parse the agent CLI's own context indicator out of the PTY stream (exact,
//      but coupled to that CLI's output — heuristic, tune against real output).
//      This is the ONLY signal an automated decision may act on — see
//      `measuredContextPercentage`.
//   2. Estimate from the bytes seen through the PTY (rough, agent-agnostic). A
//      fullscreen agent repaints its whole frame on every spinner tick, so this
//      over-counts badly and must never end a session; it feeds only the soft
//      tab gauge (`contextPercentage`), never auto-handoff / resume / retry.

import { SvelteMap } from "svelte/reactivity";

/** Rough characters-per-token ratio for the PTY-estimate fallback. */
const CHARACTERS_PER_TOKEN = 4;
/** Assumed context window (tokens) when only the estimate is available. */
const DEFAULT_CONTEXT_LIMIT = 200_000;

interface ContextSignal {
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

/** The session's context usage percent (parsed if known, else estimated), or
 *  null when nothing has been observed yet. */
export function contextPercentage(id: string): number | null {
  const signal = signals.get(id);
  if (!signal) {
    return null;
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

/** The session's context fill from the agent's OWN reported indicator (the
 *  parsed signal alone), or null when it hasn't printed one yet. Unlike
 *  `contextPercentage` this never falls back to the byte estimate — that estimate
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
export function measuredContextPercentage(id: string): number | null {
  const signal = signals.get(id);
  if (!signal) {
    return null;
  }

  return signal.parsedPercentage ?? tokensDerivedPercentage({
    signal,
    allowFallback: false
  });
}

/** Is this session's context window established — the denominator every
 *  agent-vouched reading needs? Until it is, {@link measuredContextPercentage}
 *  answers null however many tokens the agent has reported, and auto-handoff
 *  stays disarmed. The out-of-band seed below is the only source when the agent
 *  prints no window banner, and it can only answer once the agent has recorded
 *  its model on disk — so its caller re-attempts while this is false. */
export function contextWindowKnown(id: string): boolean {
  return (signals.get(id)?.windowTokens ?? null) !== null;
}

/** Seed a session's context window from an out-of-band source — the model's
 *  advertised window, looked up online — when the agent's own `(N context)`
 *  banner never reached the parser (a re-attached session whose banner was
 *  trimmed from the replay, or an agent that stopped printing one at all).
 *  Never overrides a window the banner DID supply, so a live reading always
 *  wins. */
export function seedContextWindow({ id, windowTokens }: {
  id: string;
  windowTokens: number;
}): void {
  const previous = signals.get(id) ?? EMPTY_SIGNAL;
  if (previous.windowTokens !== null) {
    return;
  }

  signals.set(id, {
    ...previous,
    windowTokens
  });
}

/** Forget a session's context when it ends. */
export function dropContext(id: string): void {
  signals.delete(id);
}
