import { formatPercent } from "@/lib/util/format";

/** The tab glyph's tooltip states only what it can vouch for: the agent's own
 *  reported percent reads as fact; the byte estimate over-counts a fullscreen
 *  agent's repaints badly (see lib/stores/agent/context), so it is labeled the
 *  rough estimate it is instead of masquerading as a measurement. */
export function contextTooltip({ measured, estimate }: {
  measured: number | null;
  estimate: number | null;
}) {
  if (measured !== null) {
    return `${formatPercent(measured)} of context window used`;
  }

  if (estimate === null) {
    return "Context window - measuring…";
  }

  return `≈${formatPercent(estimate)} of context window used - rough estimate`;
}
