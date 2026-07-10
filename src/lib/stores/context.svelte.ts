// Context-window tracking per session (SoC: shared state in lib/stores). Powers
// auto-handoff: when an agent nears its context limit we hand off to a fresh one.
//
// Two signals, preferred in order:
//   1. Parse the agent CLI's own context indicator out of the PTY stream (exact,
//      but coupled to that CLI's output — heuristic, tune against real output).
//   2. Estimate from the bytes seen through the PTY (rough, agent-agnostic
//      fallback). Deliberately conservative so it never triggers a false handoff.

import { SvelteMap } from "svelte/reactivity";

/** Rough chars-per-token for the PTY-estimate fallback. */
const CHARS_PER_TOKEN = 4;
/** Assumed context window (tokens) when only the estimate is available. */
const DEFAULT_CONTEXT_LIMIT = 200_000;

interface ContextSignal {
  /** Percent of context used, parsed from the agent's own output (0..100). */
  parsedPct: number | null;
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
const REMAINING_RE = /(\d{1,3})\s*%\s*(?:context\s*)?(?:left|remaining)|(?:left|remaining)[^%\d]{0,24}(\d{1,3})\s*%/;
const USED_RE = /(\d{1,3})\s*%\s*context|context[^%\d]{0,24}(\d{1,3})\s*%/;
const RATIO_RE = /([\d,]+)\s*(k|m)?\s*\/\s*([\d,]+)\s*(k|m)?\s*tokens/;

/** Best-effort parse of a context "percent used" from a chunk of agent output. */
function parseUsedPct(text: string): number | null {
  const lower = text.toLowerCase();

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

/** Feed a chunk of a session's PTY output through the context signals. */
export function observeContext({ id, chunk }: {
  id: string;
  chunk: string;
}): void {
  const prev = signals.get(id) ?? {
    parsedPct: null,
    chars: 0
  };
  const parsed = parseUsedPct(chunk);
  signals.set(id, {
    parsedPct: parsed ?? prev.parsedPct,
    chars: prev.chars + chunk.length
  });
}

/** The session's context usage percent (parsed if known, else estimated), or
 *  null when nothing has been observed yet. */
export function contextPct({ id, limit = DEFAULT_CONTEXT_LIMIT }: {
  id: string;
  limit?: number;
}): number | null {
  const signal = signals.get(id);
  if (!signal) {
    return null;
  }

  if (signal.parsedPct !== null) {
    return signal.parsedPct;
  }

  const tokens = signal.chars / CHARS_PER_TOKEN;
  return Math.min(100, (tokens / limit) * 100);
}

/** Forget a session's context when it ends. */
export function dropContext(id: string): void {
  signals.delete(id);
}
