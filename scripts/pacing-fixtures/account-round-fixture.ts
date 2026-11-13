// The account-round fixture: session and window reports planned into rounds.

import { planRound } from "@/lib/usage/pacing/engine/account-round";
import type { SubagentSteps, WindowReport } from "@/lib/usage/pacing/engine/account-round";

import { between, NOW, pick, random } from "./fixture-primitives";
import { gate, WRAP_UP } from "./subagent-steps-fixture";

const ACCOUNTS = ["avi@example.com", "someone@example.com"];

const WINDOW_LABELS = ["main", "window-1", "window-2"];

function sessionReportOf(id: string) {
  const isOverPace = random() > 0.45;
  return {
    id,
    name: id,
    accountKey: pick(ACCOUNTS),
    command: "claude",
    conversationId: pick([null, `${id}-conversation`]),
    reportedAt: NOW - pick([0, 0, 30_000, 120_000]),
    load: between({
      low: 0,
      high: 60,
      step: 0.5
    }),
    overPace: isOverPace
      ? {
          shed: pick([null, 0.1, 0.25, 0.3, 0.6, 0.9]),
          saving: pick([0, 0.2, 0.5, 1])
        }
      : null,
    canStepUp: random() > 0.6,
    hasRoom: random() > 0.5,
    subagents: Array.from({ length: Math.floor(random() * 4) }, (_, i) => ({
      id: `${id}-agent-${i}`,
      description: pick([null, "Explore the repo", "Run the tests"]),
      load: between({
        low: 0,
        high: 40,
        step: 0.5
      })
    }))
  };
}

function windowReportsOf() {
  const windowCount = 1 + Math.floor(random() * 3);
  return Array.from({ length: windowCount }, (_, windowIndex) => ({
    label: pick(WINDOW_LABELS) + (windowIndex === 0 ? "" : `-${windowIndex}`),
    at: NOW,
    sessions: Array.from({ length: 1 + Math.floor(random() * 3) }, (_, sessionIndex) =>
      sessionReportOf(`w${windowIndex}s${sessionIndex}`)
    )
  }));
}

/** What an earlier round left behind: some subagents already stepped, some
 *  sessions stepped a moment ago. */
function priorRound(reports: WindowReport[]) {
  const steps: SubagentSteps = {};
  const steppedAt: Record<string, number> = {};
  for (const session of reports.flatMap(report => report.sessions)) {
    if (random() > 0.7) {
      steppedAt[session.id] = NOW - pick([10_000, 60_000]);
    }

    for (const subagent of session.subagents.filter(() => random() > 0.5)) {
      steps[`${session.id}/${subagent.id}`] = pick([gate(pick([5, 10, 20, 45])), WRAP_UP]);
    }
  }

  return {
    steps,
    steppedAt
  };
}

export function roundScenarios() {
  return Array.from({ length: 24 }, () => {
    const reports = windowReportsOf();
    const { steps, steppedAt } = priorRound(reports);
    return {
      reports,
      steps,
      steppedAt,
      leader: "main",
      now: NOW,
      expected: planRound({
        reports,
        steps,
        steppedAt: new Map(Object.entries(steppedAt)),
        leader: "main",
        now: NOW
      })
    };
  });
}
