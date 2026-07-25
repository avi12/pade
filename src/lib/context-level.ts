// Context-window severity: how full an agent's context window is, read as a
// fuel gauge relative to the auto-handoff threshold (ok → warning → critical).
// Pure and unit-tested; the reactive per-session percentage lives in
// lib/stores/context. Kept apart from that store so it carries no runes and can
// be tested without a Svelte environment, and so the threshold has one home
// (auto-handoff and the session tabs both read it — DRY).

/** Percent-of-context at which the app auto-hands a session off to a fresh
 *  agent, when the user hasn't picked their own threshold (prefs.handoffPct —
 *  `effective.handoffPercentage` is the resolved value every consumer reads). Low on
 *  purpose: quality degrades long before the window is full, so cycling early
 *  keeps the agent sharp. */
export const DEFAULT_CONTEXT_HANDOFF_PERCENTAGE = 30;
/** The range the Config stepper lets the threshold move in. */
export const MINIMUM_HANDOFF_PERCENTAGE = 10;
export const MAXIMUM_HANDOFF_PERCENTAGE = 95;

// The three gauge steps. A closed set defined once so no bare severity literal
// leaks into the tabs or the theme mapping (enums over magic strings).
export const ContextLevel = {
  ok: "ok",
  warning: "warning",
  critical: "critical"
} as const;
export type ContextLevel = (typeof ContextLevel)[keyof typeof ContextLevel];

// How far toward the handoff threshold each step kicks in: at 75% of the way the
// handoff is imminent (critical), at 50% it's approaching (warning). The critical
// band has to open well before the threshold — auto-handoff fires *at* it, so a
// band that starts at 90% would flash red for only the last sliver before the
// session cycles. fraction = percentage / context handoff percentage.
const HANDOFF_IMMINENT_FRACTION = 0.75;
const HANDOFF_APPROACHING_FRACTION = 0.5;

/** Map a context-usage percent (0..100) to its severity relative to the handoff
 *  threshold: ≥75% of the way there is critical, ≥50% is warning, else ok. */
export function contextLevel({ percentage, threshold }: {
  percentage: number;
  threshold: number;
}): ContextLevel {
  const fraction = Math.min(percentage / threshold, 1);
  if (fraction >= HANDOFF_IMMINENT_FRACTION) {
    return ContextLevel.critical;
  }

  if (fraction >= HANDOFF_APPROACHING_FRACTION) {
    return ContextLevel.warning;
  }

  return ContextLevel.ok;
}
