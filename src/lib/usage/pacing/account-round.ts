// One account-wide pacing round over every live session in every PADE window,
// and every subagent those sessions run - pure, so each rule is tested on its
// own (lib/stores/usage/usageAccountRound carries it between the windows).
//
// An account's burn is one number for all of it, so its slowdown is one
// decision. Each window owns its own sessions - only it can switch a session's
// model or stop it - so each window reports what its working sessions spend
// and what their next step would save, and ONE window, the leader (the open
// window whose label sorts first among those reporting), plans the round for
// all of them:
//
//   * over pace, every candidate - each session by its own share (its running
//     subagents excluded), each running subagent by its own - offers its next
//     step and what it saves, and the steps go to the biggest share of the
//     account saved first until they cover what the account must shed
//     (`planShedding`). A session's step goes back to its own window to take;
//     a subagent's step (lib/usage/pacing/subagent-steps) is applied by the
//     leader straight to the session's pacing control file, which the agent's
//     hook reads on every tool call - whichever window the session lives in;
//   * while the account sheds past NO_SPAWN_SHED, no session on it may start
//     a subagent;
//   * with room again on every session of the account, the lightest
//     stepped-down session climbs back and each subagent's hold eases.
//
// The leader's plan is broadcast; every window takes the session steps that
// are its own. A session stepped by a round is skipped until its window reports
// again, so a report older than the step never steps it twice.

import { SubagentStep } from "@/lib/types/pacing";
import { lightestToRestore, planShedding } from "@/lib/usage/pacing/load-shedding";
import type { ShedCandidate } from "@/lib/usage/pacing/load-shedding";
import { easedSubagentStep, isBlockingSpawns, nextSubagentStep } from "@/lib/usage/pacing/subagent-steps";
import { z } from "zod";

/** The channel the windows share the round on (they share an origin). */
export const PACING_CHANNEL = "pade.usage-pacing";

export const SubagentReport = z.object({
  /** The agent's own subagent id, as its hooks report its tool calls. */
  id: z.string().min(1),
  description: z.string().nullable(),
  /** Its measured share of the account, on the sessions' scale. */
  load: z.number().nonnegative()
});

const SessionReport = z.object({
  id: z.string().min(1),
  /** The tab's name, for the usage panel's list. */
  name: z.string(),
  accountKey: z.string(),
  command: z.string(),
  conversationId: z.string().nullable(),
  /** When its window measured it. */
  reportedAt: z.number(),
  /** Its own share of the account, its running subagents excluded. */
  load: z.number().nonnegative(),
  /** Over pace by its own project's settings: what the account must shed
   *  (null while unmeasured) and what its next step saves of its own load. */
  overPace: z.object({
    shed: z.number().nullable(),
    saving: z.number().min(0).max(1)
  }).nullable(),
  /** Stepped down, with room to step back up. */
  canStepUp: z.boolean(),
  /** On pace with room to spare. */
  hasRoom: z.boolean(),
  subagents: z.array(SubagentReport)
});
export type SessionReport = z.infer<typeof SessionReport>;

const WindowReport = z.object({
  label: z.string().min(1),
  at: z.number(),
  sessions: z.array(SessionReport)
});
export type WindowReport = z.infer<typeof WindowReport>;

/** The round's subagent steps by `subagentKey` - its memory, kept where every
 *  window reads it, so a new leader takes over where the last one stopped. */
export const SubagentSteps = z.record(z.string(), SubagentStep);
export type SubagentSteps = z.infer<typeof SubagentSteps>;

/** One session's throttle as the round set it, with what the usage panel
 *  names it by. */
const ThrottledSession = z.object({
  sessionId: z.string(),
  name: z.string(),
  isBlockingSpawns: z.boolean(),
  subagents: z.array(
    z.object({
      id: z.string(),
      description: z.string().nullable(),
      step: SubagentStep
    })
  )
});
export type ThrottledSession = z.infer<typeof ThrottledSession>;

const RoundPlan = z.object({
  leader: z.string(),
  at: z.number(),
  /** Sessions to take their next step, each in its own window. */
  climb: z.array(z.string()),
  /** Stepped-down sessions to step back up. */
  restore: z.array(z.string()),
  /** Accounts a step went to: every window starts their burn rate over. */
  steppedAccounts: z.array(z.string()),
  /** Every reported session, throttled or not - an empty throttle lifts it. */
  throttles: z.array(ThrottledSession)
});
export type RoundPlan = z.infer<typeof RoundPlan>;

export const PacingMessageKind = z.enum(["report", "plan", "gone"]);
const PacingMessage = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal(PacingMessageKind.enum.report),
    report: WindowReport
  }),
  z.object({
    kind: z.literal(PacingMessageKind.enum.plan),
    plan: RoundPlan
  }),
  z.object({
    kind: z.literal(PacingMessageKind.enum.gone),
    label: z.string()
  })
]);
export type PacingMessage = z.infer<typeof PacingMessage>;

/** Another window's message is a trust boundary like any other. */
export function parsePacingMessage(raw: unknown) {
  return PacingMessage.safeParse(raw);
}

/** A window's report counts this long - a couple of its scans - so a window
 *  that closed without a goodbye stops leading and stops being planned for. */
export function reportLifetime(pollMs: number) {
  return 2.5 * pollMs;
}

/** The reports still current at `now`. */
export function freshReports({ reports, now, lifetimeMs }: {
  reports: Iterable<WindowReport>;
  now: number;
  lifetimeMs: number;
}) {
  return [...reports].filter(report => now - report.at <= lifetimeMs);
}

/** The window that plans the round: the first label among the reporting ones. */
export function roundLeader(reports: readonly Pick<WindowReport, "label">[]) {
  return reports.map(report => report.label).toSorted()[0] ?? null;
}

/** How the round keys one subagent: its session and its own id. */
export function subagentKey({ sessionId, subagentId }: {
  sessionId: string;
  subagentId: string;
}) {
  return `${sessionId}/${subagentId}`;
}

/** Candidate ids, which say whose step it is. */
const SESSION_CANDIDATE = "session:";
const SUBAGENT_CANDIDATE = "subagent:";

/** Every running subagent of an account's sessions, under its round key. */
function subagentsOf(sessions: readonly SessionReport[]) {
  return sessions.flatMap(session => session.subagents.map(subagent => ({
    key: subagentKey({
      sessionId: session.id,
      subagentId: subagent.id
    }),
    load: subagent.load
  })));
}

/** What the account must shed: the most any of its over-pace sessions asks
 *  for - null while none of them has a measured burn. */
function accountShed(sessions: readonly SessionReport[]) {
  let shed: number | null = null;
  for (const session of sessions) {
    const sessionShed = session.overPace?.shed ?? null;
    const isLargerShed = sessionShed !== null && (shed === null || sessionShed > shed);
    if (isLargerShed) {
      shed = sessionShed;
    }
  }

  return shed;
}

/** An account on pace: once every session on it has room, each hold eases and
 *  the lightest stepped-down session climbs back. */
function easeAccount({ sessions, steps }: {
  sessions: readonly SessionReport[];
  steps: SubagentSteps;
}) {
  const hasRoom = sessions.every(session => session.hasRoom);
  const nextSteps: SubagentSteps = {};
  for (const { key } of subagentsOf(sessions)) {
    const current = steps[key];
    const eased = current === undefined || !hasRoom ? current : easedSubagentStep(current);
    if (eased !== undefined && eased !== null) {
      nextSteps[key] = eased;
    }
  }

  const restoring = lightestToRestore(sessions.filter(session => session.canStepUp));
  return {
    climb: [],
    restore: restoring === null ? [] : [restoring],
    steps: nextSteps,
    isBlockingSpawns: false,
    hasStepped: restoring !== null
  };
}

/** Every step the account could take this round: each over-pace session not
 *  stepped since its report, and each subagent with a step left. */
function shedCandidates({ sessions, steps, steppedAt, shed }: {
  sessions: readonly SessionReport[];
  steps: SubagentSteps;
  steppedAt: ReadonlyMap<string, number>;
  shed: number | null;
}) {
  const candidates: ShedCandidate[] = [];
  for (const session of sessions) {
    const isStaleAfterStep = (steppedAt.get(session.id) ?? 0) > session.reportedAt;
    if (session.overPace !== null && !isStaleAfterStep) {
      candidates.push({
        id: `${SESSION_CANDIDATE}${session.id}`,
        load: session.load,
        saving: session.overPace.saving
      });
    }
  }

  const moves = new Map<string, SubagentStep>();
  for (const subagent of subagentsOf(sessions)) {
    const next = nextSubagentStep({
      current: steps[subagent.key],
      shed
    });
    if (next !== null) {
      moves.set(subagent.key, next.step);
      candidates.push({
        id: `${SUBAGENT_CANDIDATE}${subagent.key}`,
        load: subagent.load,
        saving: next.saving
      });
    }
  }

  return {
    candidates,
    moves
  };
}

/** An account over pace: the steps that save the most of it go first, until
 *  they cover its shed. */
function shedAccount({ sessions, steps, steppedAt }: {
  sessions: readonly SessionReport[];
  steps: SubagentSteps;
  steppedAt: ReadonlyMap<string, number>;
}) {
  const shed = accountShed(sessions);
  const subagents = subagentsOf(sessions);
  let totalLoad = 0;
  for (const load of [...sessions, ...subagents].map(entry => entry.load)) {
    totalLoad += load;
  }

  const { candidates, moves } = shedCandidates({
    sessions,
    steps,
    steppedAt,
    shed
  });
  const chosen = planShedding({
    candidates,
    totalLoad,
    fraction: shed
  });
  const nextSteps: SubagentSteps = {};
  for (const { key } of subagents) {
    const isChosen = chosen.includes(`${SUBAGENT_CANDIDATE}${key}`);
    const kept = isChosen ? moves.get(key) : steps[key];
    if (kept !== undefined) {
      nextSteps[key] = kept;
    }
  }

  return {
    climb: chosen.filter(id => id.startsWith(SESSION_CANDIDATE)).map(id => id.slice(SESSION_CANDIDATE.length)),
    restore: [],
    steps: nextSteps,
    isBlockingSpawns: isBlockingSpawns(shed),
    hasStepped: chosen.length > 0
  };
}

/** One account's sessions and the round's say over them. */
function planAccount({ sessions, steps, steppedAt }: {
  sessions: readonly SessionReport[];
  steps: SubagentSteps;
  steppedAt: ReadonlyMap<string, number>;
}) {
  const isOverPace = sessions.some(session => session.overPace !== null);
  if (!isOverPace) {
    return easeAccount({
      sessions,
      steps
    });
  }

  return shedAccount({
    sessions,
    steps,
    steppedAt
  });
}

/** The round over every fresh window's sessions, account by account, and the
 *  subagent steps it leaves - a subagent no longer reported loses its step. */
export function planRound({ reports, steps, steppedAt, leader, now }: {
  reports: readonly WindowReport[];
  /** The round's subagent steps so far, by `subagentKey`. */
  steps: SubagentSteps;
  /** When a round last stepped each session. */
  steppedAt: ReadonlyMap<string, number>;
  leader: string;
  now: number;
}) {
  const byAccount = new Map<string, SessionReport[]>();
  for (const report of reports) {
    for (const session of report.sessions) {
      byAccount.set(session.accountKey, [...byAccount.get(session.accountKey) ?? [], session]);
    }
  }

  const plan: RoundPlan = {
    leader,
    at: now,
    climb: [],
    restore: [],
    steppedAccounts: [],
    throttles: []
  };
  let nextSteps: SubagentSteps = {};
  for (const [accountKey, sessions] of byAccount) {
    const account = planAccount({
      sessions,
      steps,
      steppedAt
    });
    plan.climb.push(...account.climb);
    plan.restore.push(...account.restore);
    nextSteps = {
      ...nextSteps,
      ...account.steps
    };

    if (account.hasStepped) {
      plan.steppedAccounts.push(accountKey);
    }

    for (const session of sessions) {
      plan.throttles.push({
        sessionId: session.id,
        name: session.name,
        isBlockingSpawns: account.isBlockingSpawns,
        subagents: session.subagents.flatMap(subagent => {
          const step = account.steps[subagentKey({
            sessionId: session.id,
            subagentId: subagent.id
          })];
          return step === undefined ? [] : [{
            id: subagent.id,
            description: subagent.description,
            step
          }];
        })
      });
    }
  }

  return {
    plan,
    steps: nextSteps
  };
}

/** Whether a throttle asks anything of its session. */
export function isThrottling(throttle: Pick<ThrottledSession, "isBlockingSpawns" | "subagents">) {
  return throttle.isBlockingSpawns || throttle.subagents.length > 0;
}

/** A throttle as the session's control file holds it. */
export function controlThrottle(throttle: Pick<ThrottledSession, "isBlockingSpawns" | "subagents">) {
  return {
    isBlockingSpawns: throttle.isBlockingSpawns,
    subagents: Object.fromEntries(throttle.subagents.map(subagent => [subagent.id, subagent.step]))
  };
}
