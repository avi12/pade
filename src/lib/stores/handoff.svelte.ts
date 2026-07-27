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
import { dropContext, measuredContextPercentage } from "@/lib/stores/context.svelte";
import { dropSessionStatus, sessionStatus } from "@/lib/stores/sessions.svelte";
import { pastedText, PROMPT_SUBMIT, submittedPrompt } from "@/lib/terminal-input";
import { SessionStatus } from "@/lib/types";
import type { Agent, AgentSession } from "@/lib/types";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

// Generous: an agent at a near-full context writes its handoff slowly, and a
// timeout that beats the write used to strand the successor without a doc.
const HANDOFF_DOC_TIMEOUT_MS = 300_000;
const HANDOFF_SETTLE_MS = 3_000;
// How often to stat the handoff doc on disk while waiting — the reliable detector
// for a doc the change feed ignores (so the cycle proceeds seconds after the
// write, not on the full timeout).
const HANDOFF_DOC_POLL_MS = 1_000;
const USAGE_EXHAUSTED_PERCENTAGE = 95;
// After pasting the request, the submitting Enter is re-sent until the agent is
// seen working. A TUI's post-paste guard swallows the Enter that arrives in the
// same burst as the paste, so the first one often doesn't take; each retry sends
// only the Enter (never re-pastes), spaced enough for the agent to react.
const HANDOFF_SUBMIT_ATTEMPTS = 4;
const HANDOFF_SUBMIT_CONFIRM_MS = 1_500;
// How often the successor is checked for having finished its first turn (the
// doc is certainly consumed by then), and how long before we stop watching.
const SUCCESSOR_POLL_MS = 3_000;
const SUCCESSOR_DEADLINE_MS = 10 * 60_000;

export const HandoffReason = {
  ContextLimit: "context-limit",
  ConfigurationChange: "configuration-change",
  Save: "save"
} as const;
type HandoffReason = typeof HandoffReason[keyof typeof HandoffReason];

/** The one sentence that opens the handoff request, per reason — the SSOT so the
 *  wording never drifts and adding a reason is one entry, not a new branch. */
const HANDOFF_TRIGGER: Record<HandoffReason, string> = {
  [HandoffReason.ContextLimit]: "Your context window is nearly full.",
  [HandoffReason.ConfigurationChange]:
    "The project's MCP server configuration changed, so PADE must restart this agent to load it.",
  [HandoffReason.Save]:
    "This workspace is being saved as a permanent project, and its agent is restarting in the saved location."
};

/** A filesystem-safe slug for the handoff doc, from the workspace label/dir. */
export function handoffSlug(source: string): string {
  const slug = source
    .replaceAll(/[^a-z0-9-]+/gi, "-")
    .replaceAll(/^-+|-+$/g, "")
    .toLowerCase();
  return slug || "session";
}

/** The leading segment of a session's UUID — a short, stable per-session token
 *  that keeps the handoff doc name unique without dragging the whole id into a
 *  filename. Falls back to a generic token for a non-UUID id. */
function sessionToken(sessionId: string): string {
  return sessionId.split("-")[0] || "session";
}

/** The handoff doc's file name for one session. Unique PER SESSION, not just per
 *  project: two agents open in the SAME project must never share one
 *  `continue-<project>.md`. When they did, one session's doc-write fired the
 *  other session's `waitForFile` (it matches on file name alone), tearing the
 *  wrong — usually idle — session down; the project slug keeps the name
 *  human-readable, the session token keeps it collision-free. */
export function handoffDocName({ source, sessionId }: {
  source: string;
  sessionId: string;
}): string {
  return `continue-${handoffSlug(source)}-${sessionToken(sessionId)}.md`;
}

/** The handoff request's text, without any paste framing or submit keystroke —
 *  what the agent is asked to do. The store pastes this and submits it with a
 *  separate, retried Enter (see `submitHandoffRequest`). */
export function handoffRequestBody({ doc, reason }: {
  doc: string;
  reason: HandoffReason;
}): string {
  return `${HANDOFF_TRIGGER[reason]} Please write a concise handoff to ${doc} — the current state, what you've completed, and the exact next steps to continue — then stop.`;
}

export function handoffPrompt({ doc, reason }: {
  doc: string;
  reason: HandoffReason;
}): string {
  return submittedPrompt(
    handoffRequestBody({
      doc,
      reason
    })
  );
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
export function successorPrompt(doc: string): string {
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

/** Pick the successor for a specific handoff trigger. Configuration changes
 * must relaunch the governed agent itself so its fresh process reads that
 * agent's changed config; context handoffs retain quota-based crossover. */
export async function pickHandoffSuccessor({ reason, current, available, hasHeadroom }: {
  reason: HandoffReason;
  current: Agent;
  available: Agent[];
  hasHeadroom: (agentId: string) => Promise<boolean>;
}): Promise<Agent | null> {
  if (reason === HandoffReason.ConfigurationChange) {
    return current;
  }

  return await pickSuccessor({
    current,
    available,
    hasHeadroom
  });
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
  /** The percent-of-context that triggers the cycle — the resolved
   *  prefs.handoffPct (`effective.handoffPercentage`), read per scan so a Config
   *  change applies without a restart. */
  thresholdPercentage: () => number;
  /** Source text for the handoff-doc slug (workspace label or short dir). */
  slugSource: () => string;
  /** The open project's root dir — where the handoff doc lands (and is
   *  deleted from once the successor has consumed it). */
  projectDirectory: () => string;
  /** Drop an ended session from the shell's tab strip and panes. */
  removeSession: (id: string) => void;
  /** Start the successor agent seeded to continue from the handoff doc.
   *  Returns the new session's id so the doc's consumption can be watched. */
  launchSuccessor: (options: {
    agent: Agent;
    cwd?: string;
    initialPrompt: string;
  }) => string;
}

/** The auto-handoff machinery, scoped to one app shell. The shell calls
 *  `check()` from a `$effect` and `dispose()` on destroy; `note` is the status
 *  line to show while a handoff is in flight ("" when idle). */
// The in-flight marker's durable twin. The in-memory set dies with the module
// (a window reload, an HMR swap of anything in its import chain), and a scan
// running on the fresh instance re-prompted a session whose handoff request
// was already sitting in the agent's queue — the user watched the same
// request queue twice. sessionStorage survives both resets and still scopes
// to this window's lifetime, matching the sessions themselves.
const HANDOFF_MARKER_PREFIX = "pade-handoff-inflight:";

function persistentMarker(sessionId: string): string {
  return `${HANDOFF_MARKER_PREFIX}${sessionId}`;
}

export function createAutoHandoff(host: HandoffHost) {
  const handingOff = new SvelteSet<string>();
  let note = $state("");

  function markHandingOff(sessionId: string) {
    handingOff.add(sessionId);
    sessionStorage.setItem(persistentMarker(sessionId), "1");
  }

  function unmarkHandingOff(sessionId: string) {
    handingOff.delete(sessionId);
    sessionStorage.removeItem(persistentMarker(sessionId));
  }

  function isHandingOff(sessionId: string): boolean {
    return handingOff.has(sessionId) || sessionStorage.getItem(persistentMarker(sessionId)) !== null;
  }

  // How many times a session may be re-asked for its handoff doc. Each retry
  // only happens once the session reads ready again, so this bounds the
  // prompt-noise for an agent that answers without ever writing the file;
  // after the last attempt the session stays marked and keeps running.
  const HANDOFF_ATTEMPT_LIMIT = 3;
  const handoffAttempts = new SvelteMap<string, number>();

  // In-flight waitForFile resources. A handoff can pend up to 120s, so its
  // feed listener + timers must be torn down if the shell unmounts first —
  // otherwise the watcher subscription and timers leak. Tracked here so
  // dispose() can clear every still-pending wait.
  const pendingUnlistens = new SvelteSet<UnlistenFn>();
  const pendingTimers = new SvelteSet<ReturnType<typeof setTimeout>>();

  // Track one timer in the pending set and return its id, so every timer we
  // create is registered for teardown in exactly one place.
  function trackTimer(handler: () => void, delayMs: number): ReturnType<typeof setTimeout> {
    const timer = setTimeout(handler, delayMs);
    pendingTimers.add(timer);
    return timer;
  }

  // Resolve `true` once the doc named `name` exists in the project, `false` when
  // the deadline passes without it. Two detectors race: the watcher's change
  // event (fast, but SILENT for a doc the feed ignores — a gitignored
  // `continue-*.md` never emits, which used to strand the cycle on the full
  // timeout), and a direct disk poll (authoritative regardless of ignore rules).
  // Whichever sees it first wins.
  function waitForFile(name: string): Promise<boolean> {
    return new Promise(resolve => {
      const docPath = `${host.projectDirectory()}/${name}`;
      let unlisten: UnlistenFn | undefined;
      let settleTimer: ReturnType<typeof setTimeout> | undefined;
      let pollTimer: ReturnType<typeof setTimeout> | undefined;
      // Single teardown path: drop the listener + every timer from the pending
      // set, cancel them, then resolve. Used by every exit (match, settle, poll,
      // timeout).
      function finish(seen: boolean) {
        if (unlisten) {
          pendingUnlistens.delete(unlisten);
          unlisten();
        }

        for (const timer of [deadlineTimer, settleTimer, pollTimer]) {
          if (timer !== undefined) {
            pendingTimers.delete(timer);
            clearTimeout(timer);
          }
        }

        resolve(seen);
      }

      // Read by finish() only at call time (a timer fires well after this line),
      // so a const in the closure is safe.
      const deadlineTimer = trackTimer(() => finish(false), HANDOFF_DOC_TIMEOUT_MS);
      const target = name.toLowerCase();

      // Authoritative fallback: stat the doc on disk on a slow tick, so a doc the
      // watcher never reports (ignored path) is still caught within a poll.
      async function pollDisk() {
        const probe = await workspace.probePath(docPath).catch(() => null);
        if (probe?.isFile === true) {
          finish(true);
          return;
        }

        pollTimer = trackTimer(pollDisk, HANDOFF_DOC_POLL_MS);
      }

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

            settleTimer = trackTimer(() => finish(true), HANDOFF_SETTLE_MS);
          });
          pendingUnlistens.add(unlisten);
        } catch {
          // Subscription failed to arm; the disk poll and deadline still resolve.
        }
      }

      pollTimer = trackTimer(pollDisk, HANDOFF_DOC_POLL_MS);
      subscribeToChanges();
    });
  }

  // Resolve after the delay, tracking the timer so dispose() tears it down.
  function afterDelay(delayMs: number): Promise<void> {
    return new Promise(resolve => {
      trackTimer(() => resolve(), delayMs);
    });
  }

  // Deliver the request as a paste, then submit with a SEPARATE Enter — re-sent
  // until the agent starts working. A TUI guards against the Enter that
  // immediately follows a bracketed paste (so a pasted multi-line block isn't
  // submitted by accident), silently swallowing a one-shot paste+CR and leaving
  // the request unsent in the composer. Re-sending only the Enter clears the
  // guard without duplicating the pasted text.
  async function submitHandoffRequest({ session, doc, reason }: {
    session: AgentSession;
    doc: string;
    reason: HandoffReason;
  }): Promise<void> {
    await pty.write({
      id: session.id,
      data: pastedText(
        handoffRequestBody({
          doc,
          reason
        })
      )
    });

    for (let attempt = 0; attempt < HANDOFF_SUBMIT_ATTEMPTS; attempt += 1) {
      await pty.write({
        id: session.id,
        data: PROMPT_SUBMIT
      });
      await afterDelay(HANDOFF_SUBMIT_CONFIRM_MS);

      if (sessionStatus(session.id) === SessionStatus.enum.working) {
        return;
      }
    }
  }

  // Only cycle when there's quota to spare — a handoff itself costs tokens. An
  // unknown quota (tier-only) counts as "enough" so the feature still works.
  async function hasEnoughUsage(agent: string): Promise<boolean> {
    const quota = await usage.get(agent).catch(() => null);
    if (!quota || quota.usedPct == null) {
      return true;
    }

    return quota.usedPct < USAGE_EXHAUSTED_PERCENTAGE;
  }

  async function handoff({ session, reason }: {
    session: AgentSession;
    reason: HandoffReason;
  }) {
    // Same agent while it still has headroom; otherwise cross over to the first
    // other available agent that does. No agent with headroom → stay marked so we
    // don't re-check each tick; skip this cycle.
    const successorAgent = await pickHandoffSuccessor({
      reason,
      current: session.agent,
      available: host.availableAgents(),
      hasHeadroom: hasEnoughUsage
    });
    if (!successorAgent) {
      return;
    }

    const doc = handoffDocName({
      source: host.slugSource(),
      sessionId: session.id
    });
    const isCrossover = successorAgent.id !== session.agent.id;
    // The context note states the measured number: an unquantified "nearly
    // full" reads as a lie whenever the user is looking at a different (or
    // fresher) session than the one that hit the threshold.
    const measuredPercentage = Math.round(measuredContextPercentage(session.id) ?? 0);
    if (reason === HandoffReason.ConfigurationChange) {
      note = `MCP servers changed — ${session.agent.label} is writing a handoff before restarting…`;
    } else {
      note = isCrossover
        ? `${session.agent.label} is out of usage — handing off to ${successorAgent.label}…`
        : `${session.agent.label} context at ${measuredPercentage}% — handing off to a fresh agent…`;
    }

    // 1. Ask the agent to write the handoff doc, then wait for it to land —
    // unless a previous attempt already produced it: a request sent to a busy
    // agent gets queued and answered long after the wait below expired, so a
    // retry must consume the doc that late answer wrote instead of asking for
    // a second one.
    const existing = await workspace
      .probePath(`${host.projectDirectory()}/${doc}`)
      .catch(() => null);
    const alreadyWritten = existing?.isFile === true;
    if (!alreadyWritten) {
      await submitHandoffRequest({
        session,
        doc,
        reason
      });
      const watcherSawDoc = await waitForFile(doc);
      // Never cycle without the doc actually on disk: killing the session and
      // seeding a successor onto a file that was never written strands the
      // successor with nothing to read (and the conversation it was meant to
      // continue is gone). The watcher can also simply have missed the write
      // (an ignored path, a torn-down subscription), so a timeout gets one
      // direct disk probe before giving up. On a genuine no-doc, leave the
      // session running but UNMARK it: the request usually sits queued in a
      // busy agent, and the next scan that finds the session ready again
      // retries — the probe above then short-circuits onto the doc the late
      // answer wrote, so the cycle completes instead of dying here forever.
      if (!watcherSawDoc) {
        const probe = await workspace
          .probePath(`${host.projectDirectory()}/${doc}`)
          .catch(() => null);
        const docOnDisk = probe?.isFile === true;
        if (!docOnDisk) {
          const attempts = (handoffAttempts.get(session.id) ?? 0) + 1;
          handoffAttempts.set(session.id, attempts);

          if (attempts < HANDOFF_ATTEMPT_LIMIT) {
            unmarkHandingOff(session.id);
          }

          note = "";
          return;
        }
      }
    }

    // 2. End the session, 3. start the successor seeded to continue.
    const { cwd } = session;
    await pty.kill(session.id);
    host.removeSession(session.id);
    dropSessionStatus(session.id);
    dropContext(session.id);
    unmarkHandingOff(session.id);
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
      dir: host.projectDirectory(),
      name: doc
    });
  }

  // Ask `session`'s agent to write its handoff doc and resolve once the doc is on
  // disk — the reusable core of the handoff, WITHOUT the kill/launch. The
  // workspace-save flow uses it: the doc is written into the temp workspace, rides
  // along in the copy to the saved project, and the successor there is seeded to
  // continue from it. Returns the doc's file name, or null when the agent never
  // produced it (the caller falls back to a plain restart). A doc a prior attempt
  // already wrote is reused, not re-requested.
  async function writeHandoffDoc(session: AgentSession): Promise<string | null> {
    const doc = handoffDocName({
      source: host.slugSource(),
      sessionId: session.id
    });
    const docPath = `${host.projectDirectory()}/${doc}`;
    const existing = await workspace.probePath(docPath).catch(() => null);
    if (existing?.isFile === true) {
      return doc;
    }

    await submitHandoffRequest({
      session,
      doc,
      reason: HandoffReason.Save
    });
    const seen = await waitForFile(doc);
    if (seen) {
      return doc;
    }

    const probe = await workspace.probePath(docPath).catch(() => null);
    return probe?.isFile === true ? doc : null;
  }

  // Fire-and-forget entry point for the scan and the force path. handoff is
  // best-effort: swallow any failure (including deleting a doc the agent never
  // wrote on the timeout path) and clear the in-flight marker + note so a later
  // scan can retry.
  async function runHandoff({ session, reason }: {
    session: AgentSession;
    reason: HandoffReason;
  }) {
    try {
      await handoff({
        session,
        reason
      });
    } catch {
      unmarkHandingOff(session.id);
      note = "";
    }
  }

  // Resolve once the successor has genuinely worked its first turn and gone ready
  // — or the deadline passes, or it disappears. The doc is deleted the moment this
  // resolves, so the "worked" test must be the REAL doc-reading turn, not a
  // transient blip: a fresh session flips to `working` for a beat on boot, and
  // again on each initial-prompt paste-echo, then settles back to `ready` within
  // one poll. Counting a single such blip as "first turn done" deleted the handoff
  // doc while the successor was still trying to read it — the reported "agent
  // couldn't retrieve the file". So only SUSTAINED work — `working` on two
  // consecutive polls — arms the settle; a doc-reading turn stays working across
  // polls, while a boot/paste blip never spans two. A fast turn that we miss simply
  // falls to the deadline, which deletes the doc long after it was safely consumed
  // — late cleanup, never an early delete.
  function waitForSuccessorSettled(id: string): Promise<void> {
    return new Promise(resolve => {
      let consecutiveWorking = 0;
      let sawSustainedWork = false;
      const startedAt = Date.now();
      function poll() {
        const status = sessionStatus(id);
        const gone = !host.sessions().some(session => session.id === id);
        const expired = Date.now() - startedAt > SUCCESSOR_DEADLINE_MS;
        if (status === SessionStatus.enum.working) {
          consecutiveWorking += 1;
          if (consecutiveWorking >= 2) {
            sawSustainedWork = true;
          }
        } else {
          consecutiveWorking = 0;
        }

        const settled = sawSustainedWork && status === SessionStatus.enum.ready;
        if (settled || gone || expired) {
          resolve();
          return;
        }

        trackTimer(poll, SUCCESSOR_POLL_MS);
      }

      trackTimer(poll, SUCCESSOR_POLL_MS);
    });
  }

  // Scan for sessions near the context limit and kick off their handoff.
  function check() {
    if (host.isOptedOut()) {
      return;
    }

    for (const session of host.sessions()) {
      const percentage = measuredContextPercentage(session.id);
      const nearLimit = percentage !== null && percentage >= host.thresholdPercentage();
      const idle = sessionStatus(session.id) === SessionStatus.enum.ready;
      const already = isHandingOff(session.id);
      if (!nearLimit || !idle || already) {
        continue;
      }

      markHandingOff(session.id);
      runHandoff({
        session,
        reason: HandoffReason.ContextLimit
      });
    }
  }

  // Hand a session off right now — the usage-resume flow calls this at window
  // reset when the context is too full to just continue. Same single-flight
  // guard as the scan, none of its idle/threshold gates: the caller has
  // already decided this session must cycle.
  function force(session: AgentSession) {
    if (isHandingOff(session.id)) {
      return;
    }

    markHandingOff(session.id);
    runHandoff({
      session,
      reason: HandoffReason.ContextLimit
    });
  }

  // An MCP membership change must launch the same governed agent again so it
  // reads the new configuration. Unlike a context-limit handoff, this bypasses
  // quota-based crossover while retaining the document-first safety sequence.
  async function restartForConfiguration(session: AgentSession) {
    if (isHandingOff(session.id)) {
      return;
    }

    markHandingOff(session.id);
    await runHandoff({
      session,
      reason: HandoffReason.ConfigurationChange
    });
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
    restartForConfiguration,
    writeHandoffDoc,
    dispose
  };
}
