// Context-window severity: how full an agent's context window is, read as a
// fuel gauge relative to the auto-handoff threshold (ok → warning → critical).
// Pure and unit-tested; the reactive per-session percentage lives in
// lib/stores/context. Kept apart from that store so it carries no runes and can
// be tested without a Svelte environment, and so the threshold has one home
// (auto-handoff and the session tabs both read it — DRY).

/** Percent-of-context at which the app auto-hands a session off to a fresh
 *  agent, when the user hasn't picked their own threshold (prefs.handoffPct —
 *  `effective.handoffPct` is the resolved value every consumer reads). Low on
 *  purpose: quality degrades long before the window is full, so cycling early
 *  keeps the agent sharp. */
export const DEFAULT_CONTEXT_HANDOFF_PCT = 30;
/** The range the Config stepper lets the threshold move in. */
export const MINIMUM_HANDOFF_PCT = 10;
export const MAXIMUM_HANDOFF_PCT = 95;

// The three gauge steps. A closed set defined once so no bare severity literal
// leaks into the tabs or the theme mapping (enums over magic strings).
export const ContextLevel = {
  ok: "ok",
  warning: "warning",
  critical: "critical"
} as const;
export type ContextLevel = (typeof ContextLevel)[keyof typeof ContextLevel];

// How far toward the handoff threshold each step kicks in: at 90% of the way the
// handoff is imminent (critical), at 60% it's approaching (warning). Mirrors the
// design's ctxColor ramp (f = pct / CONTEXT_HANDOFF_PCT).
const HANDOFF_IMMINENT_FRACTION = 0.9;
const HANDOFF_APPROACHING_FRACTION = 0.6;

/** Map a context-usage percent (0..100) to its severity relative to the handoff
 *  threshold: ≥90% of the way there is critical, ≥60% is warning, else ok. */
export function contextLevel({ pct, threshold }: {
  pct: number;
  threshold: number;
}): ContextLevel {
  const fraction = Math.min(pct / threshold, 1);
  if (fraction >= HANDOFF_IMMINENT_FRACTION) {
    return ContextLevel.critical;
  }

  if (fraction >= HANDOFF_APPROACHING_FRACTION) {
    return ContextLevel.warning;
  }

  return ContextLevel.ok;
}
