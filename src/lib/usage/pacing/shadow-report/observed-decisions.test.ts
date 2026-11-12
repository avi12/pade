import { createObservedRecorder } from "@/lib/usage/pacing/shadow-report/observed-decisions";
import { ObservedKind } from "@/lib/usage/pacing/shadow-report/pacing-report";
import { describe, expect, it } from "vitest";

const NOW = 1_000;

describe("createObservedRecorder", () => {
  it("hands over what was decided, then starts empty", () => {
    const recorder = createObservedRecorder();
    recorder.note({
      sessionId: "a",
      kind: ObservedKind.enum.rest,
      at: NOW
    });
    expect(recorder.drain(NOW + 1)).toEqual([{
      sessionId: "a",
      kind: ObservedKind.enum.rest,
      at: NOW
    }]);
    expect(recorder.drain(NOW + 2)).toEqual([]);
  });

  it("reports none for a session it judged and took no step on", () => {
    const recorder = createObservedRecorder();
    recorder.markJudged("a");
    recorder.markJudged("b");
    recorder.note({
      sessionId: "b",
      kind: ObservedKind.enum.stop,
      at: NOW
    });
    expect(recorder.drain(NOW + 5)).toEqual([
      {
        sessionId: "b",
        kind: ObservedKind.enum.stop,
        at: NOW
      },
      {
        sessionId: "a",
        kind: ObservedKind.enum.none,
        at: NOW + 5
      }
    ]);
  });

  it("forgets who was judged once drained, so a quiet session is not reported twice", () => {
    const recorder = createObservedRecorder();
    recorder.markJudged("a");
    recorder.drain(NOW);
    expect(recorder.drain(NOW + 1)).toEqual([]);
  });

  it("keeps several decisions on one session, in the order they were taken", () => {
    const recorder = createObservedRecorder();
    recorder.note({
      sessionId: "a",
      kind: ObservedKind.enum.stepEffortDown,
      at: NOW
    });
    recorder.note({
      sessionId: "a",
      kind: ObservedKind.enum.stepModelDown,
      at: NOW + 1
    });
    expect(recorder.drain(NOW + 2).map(decision => decision.kind)).toEqual([
      ObservedKind.enum.stepEffortDown,
      ObservedKind.enum.stepModelDown
    ]);
  });
});
