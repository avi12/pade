import { ObservedKind } from "@/lib/usage/pacing/pacing-report";
import type { ObservedDecision } from "@/lib/usage/pacing/pacing-report";

/** What the pacing loop decided, collected as it goes and handed over with the
 *  next report. A session the loop judged and took no step on reports `none`,
 *  so the engine's own "nothing to do" has something to be compared with. */
export function createObservedRecorder() {
  const decisions: ObservedDecision[] = [];
  const judgedIds = new Set<string>();

  return {
    note({ sessionId, kind, at }: ObservedDecision) {
      decisions.push({
        sessionId,
        kind,
        at
      });
    },
    markJudged(sessionId: string) {
      judgedIds.add(sessionId);
    },
    /** Everything noted since the last drain, emptied. */
    drain(now: number) {
      const decidedIds = new Set(decisions.map(decision => decision.sessionId));
      const idle = [...judgedIds]
        .filter(sessionId => !decidedIds.has(sessionId))
        .map(sessionId => ({
          sessionId,
          kind: ObservedKind.enum.none,
          at: now
        }));
      const drained = [...decisions, ...idle];
      decisions.length = 0;
      judgedIds.clear();
      return drained;
    }
  };
}
