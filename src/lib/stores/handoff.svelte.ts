// Auto-handoff: when an agent nears its context window, ask it to write a
// continue-<slug>.md handoff doc, end the session, and start a fresh successor
// seeded to resume from that doc. The successor is the SAME agent while it still
// has usage headroom (a context-driven handoff); when the current agent is
// tapped out, the handoff crosses over to the first other available agent that
// does (Claude→Codex, generalized to any agents). Opt-out via prefs.autoHandoff;
// fires once per session. This module owns the machinery — thresholds, successor
// selection, the settle-wait for the doc, resource teardown — while the app
// shell supplies its session list, its available-agent pool and launch through
// `HandoffHost` and drives the scan from a component `$effect`.

import { feed, pty, usage, workspace } from "@/lib/bridge";
import { CONTEXT_HANDOFF_PCT } from "@/lib/context-level";
import { dropContext, measuredContextPct } from "@/lib/stores/context.svelte";
import { dropSessionStatus, sessionStatus } from "@/lib/stores/sessions.svelte";
import { SessionStatus } from "@/lib/types";
import type { Agent, AgentSession } from "@/lib/types";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { SvelteSet } from "svelte/reactivity";

const HANDOFF_DOC_TIMEOUT_MS = 120_000;
const HANDOFF_SETTLE_MS = 3_000;
const USAGE_EXHAUSTED_PCT = 95;
// How often the successor is checked for having finished its first turn (the
// doc is certainly consumed by then), and how long before we stop watching.
const SUCCESSOR_POLL_MS = 3_000;
const SUCCESSOR_DEADLINE_MS = 10 * 60_000;

/** A filesystem-safe slug for the handoff doc, from the workspace label/dir. */
export function handoffSlug(source: string): string {
  const slug = source
    .replaceAll(/[^a-z0-9-]+/gi, "-")
    .replaceAll(/^-+|-+$/g, "")
    .toLowerCase();
  return slug || "session";
}

function handoffPrompt(doc: string): string {
  return `\nYour context window is nearly full. Please write a concise handoff to ${doc} — the current state, what you've completed, and the exact next steps to continue — then stop.\r`;
}

/** Seed for the fresh successor: read ONLY the handoff doc and continue. The
 *  whole point of a handoff is a small, clean context — so it deliberately does
 *  NOT ask the successor to also read CLAUDE.md or earlier continue-*.md files.
 *  The agent auto-loads its project memory (CLAUDE.md) on its own, and re-reading
 *  stale handoffs would bloat the very context the handoff exists to reset. The
 *  doc must therefore be self-sufficient: current state + exact next steps.
 *  Agent-agnostic — it names a file on disk, not any one agent's memory system.
 *
 *  No trailing carriage return: this rides in as the successor session's
 *  initialPrompt, and the terminal's initial-prompt delivery appends the
 *  submitting ENTER itself (see panels/Terminal.svelte, lib/initial-prompt). */
function successorPrompt(doc: string): string {
  return `Read ${doc} to continue the work where the previous session left off.`;
}

/** Pick the agent that should take over. The current agent stays on while it
 *  still has usage headroom (the context-driven handoff — context near full but
 *  the agent can keep going). Once it's out of headroom, the first OTHER
 *  available agent that has headroom takes over (a usage crossover). `null` when
 *  no agent has headroom — the caller stays marked and skips this cycle. */
export async function pickSuccessor({ current, available, hasHeadroom }: {
  current: Agent;
  available: Agent[];
  hasHeadroom: (agentId: string) => Promise<boolean>;
}): Promise<Agent | null> {
  if (await hasHeadroom(current.id)) {
    return current;
  }

  for (const agent of available) {
    if (agent.id === current.id) {
      continue;
    }

    if (await hasHeadroom(agent.id)) {
      return agent;
    }
  }

  return null;
}

/** What the app shell provides. The reads run inside the shell's `$effect`, so
 *  the scan re-runs as the session list / prefs / context stores change. */
export interface HandoffHost {
  sessions: () => AgentSession[];
  /** The agents installed and available to take over — the crossover pool for a
   *  usage failover. The current agent is excluded at selection time. */
  availableAgents: () => Agent[];
  /** Whether the user opted out via prefs.autoHandoff. */
  isOptedOut: () => boolean;
  /** Source text for the handoff-doc slug (workspace label or short dir). */
  slugSource: () => string;
  /** The open project's root dir — where the handoff doc lands (and is
   *  deleted from once the successor has consumed it). */
  projectDir: () => string;
  /** Drop an ended session from the shell's tab strip and panes. */
  removeSession: (id: string) => void;
  /** Start the successor agent seeded to continue from the handoff doc.
   *  Returns the new session's id so the doc's consumption can be watched. */
  launchSuccessor: (opts: {
    agent: Agent;
    cwd?: string;
    initialPrompt: string;
  }) => string;
}

/** The auto-handoff machinery, scoped to one app shell. The shell calls
 *  `check()` from a `$effect` and `dispose()` on destroy; `note` is the status
 *  line to show while a handoff is in flight ("" when idle). */
export function createAutoHandoff(host: HandoffHost) {
  const handingOff = new SvelteSet<string>();
  let note = $state("");

  // In-flight waitForFile resources. A handoff can pend up to 120s, so its
  // feed listener + timers must be torn down if the shell unmounts first —
  // otherwise the watcher subscription and timers leak. Tracked here so
  // dispose() can clear every still-pending wait.
  const pendingUnlistens = new SvelteSet<UnlistenFn>();
  const pendingTimers = new SvelteSet<ReturnType<typeof setTimeout>>();

  // Track one timer in the pending set and return its id, so every timer we
  // create is registered for teardown in exactly one place.
  function trackTimer({ handler, delayMs }: {
    delayMs: number;
    handler: () => void;
  }): ReturnType<typeof setTimeout> {
    const timer = setTimeout(handler, delayMs);
    pendingTimers.add(timer);
    return timer;
  }

  // Resolve once the watcher sees `name` written (plus a short settle) or on timeout.
  function waitForFile(name: string): Promise<void> {
    return new Promise(resolve => {
      let unlisten: UnlistenFn | undefined;
      let settleTimer: ReturnType<typeof setTimeout> | undefined;
      // Single teardown path: drop the listener + both timers from the pending
      // set, cancel them, then resolve. Used by every exit (match, settle, timeout).
      function finish() {
        if (unlisten) {
          pendingUnlistens.delete(unlisten);
          unlisten();
        }

        for (const timer of [deadlineTimer, settleTimer]) {
          if (timer !== undefined) {
            pendingTimers.delete(timer);
            clearTimeout(timer);
          }
        }

        resolve();
      }

      // Read by finish() only at call time (a timer fires well after this line),
      // so a const in the closure is safe.
      const deadlineTimer = trackTimer({
        handler: finish,
        delayMs: HANDOFF_DOC_TIMEOUT_MS
      });
      const target = name.toLowerCase();

      // Kick off the async watcher subscription from this sync Promise executor.
      // It owns its own error handling, so the deadline timer still resolves the
      // wait even if the subscription never lands.
      async function subscribeToChanges() {
        try {
          unlisten = await feed.onChange(event => {
            const seen = event.path.replaceAll("\\", "/").toLowerCase().endsWith(target);
            if (!seen) {
              return;
            }

            // Restart the short settle window on each matching change; finish only
            // fires once it goes quiet (or the deadline hits first).
            if (settleTimer !== undefined) {
              pendingTimers.delete(settleTimer);
              clearTimeout(settleTimer);
            }

            settleTimer = trackTimer({
              handler: finish,
              delayMs: HANDOFF_SETTLE_MS
            });
          });
          pendingUnlistens.add(unlisten);
        } catch {
          // Subscription failed to arm; the deadline timer resolves the wait.
        }
      }

      subscribeToChanges();
    });
  }

  // Only cycle when there's quota to spare — a handoff itself costs tokens. An
  // unknown quota (tier-only) counts as "enough" so the feature still works.
  async function hasEnoughUsage(agent: string): Promise<boolean> {
    const quota = await usage.get(agent).catch(() => null);
    if (!quota || quota.usedPct == null) {
      return true;
    }

    return quota.usedPct < USAGE_EXHAUSTED_PCT;
  }

  async function handoff(session: AgentSession) {
    // Same agent while it still has headroom; otherwise cross over to the first
    // other available agent that does. No agent with headroom → stay marked so we
    // don't re-check each tick; skip this cycle.
    const successorAgent = await pickSuccessor({
      current: session.agent,
      available: host.availableAgents(),
      hasHeadroom: hasEnoughUsage
    });
    if (!successorAgent) {
      return;
    }

    const doc = `continue-${handoffSlug(host.slugSource())}.md`;
    const isCrossover = successorAgent.id !== session.agent.id;
    note = isCrossover
      ? `${session.agent.label} is out of usage — handing off to ${successorAgent.label}…`
      : `Context nearly full — handing ${session.agent.label} off to a fresh agent…`;

    // 1. Ask the agent to write the handoff doc, then wait for it to land.
    await pty.write({
      id: session.id,
      data: handoffPrompt(doc)
    });
    await waitForFile(doc);

    // 2. End the session, 3. start the successor seeded to continue.
    const { cwd } = session;
    await pty.kill(session.id);
    host.removeSession(session.id);
    dropSessionStatus(session.id);
    dropContext(session.id);
    handingOff.delete(session.id);
    const successorId = host.launchSuccessor({
      agent: successorAgent,
      cwd,
      initialPrompt: successorPrompt(doc)
    });
    note = "";

    // 4. The doc's job ends with the handoff: once the successor has finished
    // its first turn (it has certainly read the doc by then), delete it so
    // consumed handoffs never litter the project.
    await waitForSuccessorSettled(successorId);
    await workspace.deleteHandoffDoc({
      dir: host.projectDir(),
      name: doc
    });
  }

  // Fire-and-forget entry point for the scan and the force path. handoff is
  // best-effort: swallow any failure (including deleting a doc the agent never
  // wrote on the timeout path) and clear the in-flight marker + note so a later
  // scan can retry.
  async function runHandoff(session: AgentSession) {
    try {
      await handoff(session);
    } catch {
      handingOff.delete(session.id);
      note = "";
    }
  }

  // Resolve once the successor session has been seen working and then gone
  // ready — its first turn is over — or the deadline passes, or it disappears.
  function waitForSuccessorSettled(id: string): Promise<void> {
    return new Promise(resolve => {
      let sawWorking = false;
      const startedAt = Date.now();
      function poll() {
        const status = sessionStatus(id);
        const gone = !host.sessions().some(s => s.id === id);
        const expired = Date.now() - startedAt > SUCCESSOR_DEADLINE_MS;
        if (status === SessionStatus.enum.working) {
          sawWorking = true;
        }

        const settled = sawWorking && status === SessionStatus.enum.ready;
        if (settled || gone || expired) {
          resolve();
          return;
        }

        trackTimer({
          handler: poll,
          delayMs: SUCCESSOR_POLL_MS
        });
      }

      trackTimer({
        handler: poll,
        delayMs: SUCCESSOR_POLL_MS
      });
    });
  }

  // Scan for sessions near the context limit and kick off their handoff.
  function check() {
    if (host.isOptedOut()) {
      return;
    }

    for (const session of host.sessions()) {
      const pct = measuredContextPct(session.id);
      const nearLimit = pct !== null && pct >= CONTEXT_HANDOFF_PCT;
      const idle = sessionStatus(session.id) === SessionStatus.enum.ready;
      const already = handingOff.has(session.id);
      if (nearLimit && idle && !already) {
        handingOff.add(session.id);
        runHandoff(session);
      }
    }
  }

  // Hand a session off right now — the usage-resume flow calls this at window
  // reset when the context is too full to just continue. Same single-flight
  // guard as the scan, none of its idle/threshold gates: the caller has
  // already decided this session must cycle.
  function force(session: AgentSession) {
    if (handingOff.has(session.id)) {
      return;
    }

    handingOff.add(session.id);
    runHandoff(session);
  }

  // Tear down every still-pending wait (listener + timers).
  function dispose() {
    for (const unlisten of pendingUnlistens) {
      unlisten();
    }

    for (const timer of pendingTimers) {
      clearTimeout(timer);
    }

    pendingUnlistens.clear();
    pendingTimers.clear();
  }

  return {
    /** Status line shown while a handoff is in flight ("" when idle). */
    get note() {
      return note;
    },
    check,
    force,
    dispose
  };
}
