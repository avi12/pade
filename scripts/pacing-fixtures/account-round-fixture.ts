// The account-round fixture: session and window reports planned into rounds.

import { planRound } from "@/lib/usage/pacing/engine/account-round";
import type { SubagentSteps, WindowReport } from "@/lib/usage/pacing/engine/account-round";

import { between, NOW, pick, random } from "./fixture-primitives";
import { gate, WRAP_UP } from "./subagent-steps-fixture";

const ACCOUNTS = ["avi@example.com", "someone@example.com"];

const RECENT_REPORT_AGE = 30_000;

const STALE_REPORT_AGE = 120_000;

const MAX_SESSION_LOAD = 60;

const MAX_SUBAGENT_LOAD = 40;

const LOAD_STEP = 0.5;

const EVEN_ODDS = 0.5;

const STEP_UP_THRESHOLD = 0.6;

const PRIOR_STEP_THRESHOLD = 0.7;

const MAX_SUBAGENTS = 4;

const MAX_SESSIONS_PER_WINDOW = 3;

const MAX_WINDOWS = 3;

const ROUND_SCENARIO_COUNT = 24;

const SHED_FRACTIONS = [null, 0.1, 0.25, 0.3, 0.6, 0.9];

const SAVING_FRACTIONS = [0, 0.2, 0.5, 1];

const PRIOR_STEP_AGES = [10_000, 60_000];

const GATE_PERCENTAGES = [5, 10, 20, 45];

const WINDOW_LABELS = ["main", "window-1", "window-2"];

function sessionReportOf(id: string) {
  const isOverPace = random() > 0.45;
  return {
    id,
    name: id,
    accountKey: pick(ACCOUNTS),
    command: "claude",
    conversationId: pick([null, `${id}-conversation`]),
    reportedAt: NOW - pick([0, 0, RECENT_REPORT_AGE, STALE_REPORT_AGE]),
    load: between({
      low: 0,
      high: MAX_SESSION_LOAD,
      step: LOAD_STEP
    }),
    overPace: isOverPace
      ? {
          shed: pick(SHED_FRACTIONS),
          saving: pick(SAVING_FRACTIONS)
        }
      : null,
    canStepUp: random() > STEP_UP_THRESHOLD,
    hasRoom: random() > EVEN_ODDS,
    subagents: Array.from({ length: Math.floor(random() * MAX_SUBAGENTS) }, (_, i) => ({
      id: `${id}-agent-${i}`,
      description: pick([null, "Explore the repo", "Run the tests"]),
      load: between({
        low: 0,
        high: MAX_SUBAGENT_LOAD,
        step: LOAD_STEP
      })
    }))
  };
}

function windowReportsOf() {
  const windowCount = 1 + Math.floor(random() * MAX_WINDOWS);
  return Array.from({ length: windowCount }, (_, windowIndex) => ({
    label: pick(WINDOW_LABELS) + (windowIndex === 0 ? "" : `-${windowIndex}`),
    at: NOW,
    sessions: Array.from({ length: 1 + Math.floor(random() * MAX_SESSIONS_PER_WINDOW) }, (_, sessionIndex) =>
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
    if (random() > PRIOR_STEP_THRESHOLD) {
      steppedAt[session.id] = NOW - pick(PRIOR_STEP_AGES);
    }

    for (const subagent of session.subagents.filter(() => random() > EVEN_ODDS)) {
      steps[`${session.id}/${subagent.id}`] = pick([gate(pick(GATE_PERCENTAGES)), WRAP_UP]);
    }
  }

  return {
    steps,
    steppedAt
  };
}

export function roundScenarios() {
  return Array.from({ length: ROUND_SCENARIO_COUNT }, () => {
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
