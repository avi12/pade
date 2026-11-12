// Auto-handoff: when an agent nears its context window, ask it to write a
// continue-<slug>.md handoff doc, end the session, and start a fresh successor
// seeded to resume from that doc. The successor is the SAME agent while it still
// has usage headroom (a context-driven handoff); when the current agent is
// tapped out, the handoff crosses over to the first other available agent that
// does (Claude→Codex, generalized to any agents). Opt-out via prefs.autoHandoff;
// fires once per session. This module owns the machinery - thresholds, successor
// selection, the settle-wait for the doc, resource teardown - while the app
// shell supplies its session list, its available-agent pool and launch through
// `HandoffHost` and drives the scan from a component `$effect`.
//
// A handoff ends the session, and with it everything the agent still has running
// in the background - a dynamic workflow, background agents. So no handoff starts,
// and none ends a session, while the agent itself reports work in flight
// (lib/stores/agent/agentActivity); it waits for that work to finish, however long.
// The one exception is an agent about to compact its own conversation: waiting
// for its turn to end would let the compaction run first and lose the detail a
// handoff exists to keep, so the request goes in mid-turn - the agent picks it
// up between steps - and the session ends once the doc is on disk.

import { submitPastedPrompt } from "@/lib/agent/submit-prompt";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { agentReportsBusy, whenAgentSettled } from "@/lib/stores/agent/agentActivity.svelte";
import { isFileOnDisk, waitForFile } from "@/lib/stores/agent/handoff/handoff-doc-wait";
import { waitForFirstTurn } from "@/lib/stores/agent/handoff/handoff-first-turn";
import type { HandoffHost } from "@/lib/stores/agent/handoff/handoff-host";
import {
  clearPersistentMarker,
  hasPersistentMarker,
  storePersistentMarker
} from "@/lib/stores/agent/handoff/handoff-markers";
import { handingOffNote, waitingForWorkNote } from "@/lib/stores/agent/handoff/handoff-notes";
import {
  handoffDocName,
  HandoffReason,
  handoffRequestBody,
  successorPrompt
} from "@/lib/stores/agent/handoff/handoff-prompts";
import { ScanVerdict, scanVerdict } from "@/lib/stores/agent/handoff/handoff-scan";
import { hasEnoughUsage, pickHandoffSuccessor } from "@/lib/stores/agent/handoff/handoff-successor";
import { createHandoffTimers } from "@/lib/stores/agent/handoff/handoff-timers";
import { shieldHandoffSuccessor, takeOverPacing } from "@/lib/stores/usage/usagePacing.svelte";
import type { AgentSession } from "@/lib/types/agent";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

// How many times a session may be re-asked for its handoff doc. Each retry
// only happens once the session reads ready again, so this bounds the
// prompt-noise for an agent that answers without ever writing the file;
// after the last attempt the session stays marked and keeps running.
const HANDOFF_ATTEMPT_LIMIT = 3;
/** How often a session out of attempts is checked for a doc that turned up late. */
const LATE_DOC_POLL_MS = 15_000;

/** The auto-handoff machinery, scoped to one app shell. The shell calls
 *  `check()` from a `$effect` and `dispose()` on destroy; `note` is the status
 *  line to show while a handoff is in flight ("" when idle). */
export function createAutoHandoff(host: HandoffHost) {
  const handingOff = new SvelteSet<string>();
  let note = $state("");
  // The scan's own line for a session over threshold that it holds back because
  // the agent still has work running; an in-flight handoff's `note` wins over it.
  let deferredNote = $state("");

  function markHandingOff(sessionId: string) {
    handingOff.add(sessionId);
    storePersistentMarker(sessionId);
  }

  function unmarkHandingOff(sessionId: string) {
    handingOff.delete(sessionId);
    clearPersistentMarker(sessionId);
  }

  function isHandingOff(sessionId: string) {
    return handingOff.has(sessionId) || hasPersistentMarker(sessionId);
  }

  // Give up on this handoff for now, leaving the session free for a later scan.
  function abandonHandoff(sessionId: string) {
    unmarkHandingOff(sessionId);
    note = "";
  }

  function docNameFor(sessionId: string) {
    return handoffDocName({
      source: host.slugSource(),
      sessionId
    });
  }

  const handoffAttempts = new SvelteMap<string, number>();
  const timers = createHandoffTimers();
  const { trackTimer } = timers;

  function docPathOf({ session, name }: {
    session: AgentSession;
    name: string;
  }) {
    return `${host.projectDirectory(session)}/${name}`;
  }

  async function submitHandoffRequest({ session, doc, reason }: {
    session: AgentSession;
    doc: string;
    reason: HandoffReason;
  }) {
    // Pacing may be stopping the session as it fills up: a refused write would
    // lose the handoff doc.
    await takeOverPacing(session.id);
    await submitPastedPrompt({
      id: session.id,
      // The full path, never the bare name: an agent resolves a bare name
      // against its own working directory - a workspace member, a worktree, a
      // folder it picks - while PADE looks for the doc in the project root.
      text: handoffRequestBody({
        doc: docPathOf({
          session,
          name: doc
        }),
        reason
      }),
      wait: timers.afterDelay
    });
  }

  // Hold the handoff until the agent reports no work in flight, so ending the
  // session never severs a workflow or background agent it is still running.
  // Resolves whether the session is still here to hand off.
  async function waitForRunningWork({ session, reason }: {
    session: AgentSession;
    reason: HandoffReason;
  }) {
    if (agentReportsBusy(session.id)) {
      note = waitingForWorkNote({
        session,
        reason
      });
      await whenAgentSettled(session.id);
    }

    return isStillOpen(session.id);
  }

  // The same gate, skipped when the handoff has to beat the agent's own
  // compaction: then the running turn is exactly what it must not wait for.
  async function waitForRunningWorkUnlessMidTurn({ session, reason, isMidTurn }: {
    session: AgentSession;
    reason: HandoffReason;
    isMidTurn: boolean;
  }) {
    if (isMidTurn) {
      return isStillOpen(session.id);
    }

    return waitForRunningWork({
      session,
      reason
    });
  }

  function isStillOpen(sessionId: string) {
    return host.sessions().some(candidate => candidate.id === sessionId);
  }

  // Ask for the handoff doc and resolve whether it is on disk - unless a
  // previous attempt already produced it: a request sent to a busy agent gets
  // queued and answered long after the wait below expired, so a retry must
  // consume the doc that late answer wrote instead of asking for a second one.
  async function askForDoc({ session, doc, reason }: {
    session: AgentSession;
    doc: string;
    reason: HandoffReason;
  }) {
    const docPath = docPathOf({
      session,
      name: doc
    });
    if (await isFileOnDisk(docPath)) {
      return true;
    }

    await submitHandoffRequest({
      session,
      doc,
      reason
    });

    // The watcher can simply have missed the write (an ignored path, a
    // torn-down subscription), so a timeout gets one direct disk probe before
    // giving up.
    return await waitForFile({
      timers,
      isStillOpen,
      sessionId: session.id,
      docPath,
      name: doc
    }) || await isFileOnDisk(docPath);
  }

  // Never cycle without the doc actually on disk: killing the session and
  // seeding a successor onto a file that was never written strands the
  // successor with nothing to read (and the conversation it was meant to
  // continue is gone). On a genuine no-doc, leave the session running but
  // UNMARK it: the request usually sits queued in a busy agent, and the next
  // scan that finds the session ready again retries - askForDoc's first probe
  // then short-circuits onto the doc the late answer wrote, so the cycle
  // completes instead of dying here forever.
  async function requestHandoffDoc({ session, doc, reason }: {
    session: AgentSession;
    doc: string;
    reason: HandoffReason;
  }) {
    if (await askForDoc({
      session,
      doc,
      reason
    })) {
      return true;
    }

    if (!isStillOpen(session.id)) {
      abandonHandoff(session.id);
      return false;
    }

    const attempts = (handoffAttempts.get(session.id) ?? 0) + 1;
    handoffAttempts.set(session.id, attempts);

    if (attempts < HANDOFF_ATTEMPT_LIMIT) {
      unmarkHandingOff(session.id);
    } else {
      watchForLateDoc({
        session,
        doc,
        reason
      });
    }

    return false;
  }

  // Out of attempts, the session stays marked and is never asked again - but the
  // doc can still appear: an answer queued behind a long turn, or the user moving
  // a doc the agent wrote somewhere else into place. A slow disk look picks it up
  // and finishes the handoff, as long as the session is still open.
  function watchForLateDoc({ session, doc, reason }: {
    session: AgentSession;
    doc: string;
    reason: HandoffReason;
  }) {
    const docPath = docPathOf({
      session,
      name: doc
    });
    async function look() {
      if (!isStillOpen(session.id)) {
        return;
      }

      if (!await isFileOnDisk(docPath)) {
        trackTimer({
          handler: look,
          delayMs: LATE_DOC_POLL_MS
        });
        return;
      }

      handoffAttempts.delete(session.id);
      await runHandoff({
        session,
        reason
      });
    }

    trackTimer({
      handler: look,
      delayMs: LATE_DOC_POLL_MS
    });
  }

  async function handoff({ session, reason, isMidTurn }: {
    session: AgentSession;
    reason: HandoffReason;
    isMidTurn: boolean;
  }) {
    const isStillHereBeforeRequest = await waitForRunningWorkUnlessMidTurn({
      session,
      reason,
      isMidTurn
    });
    if (!isStillHereBeforeRequest) {
      abandonHandoff(session.id);
      return;
    }

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

    const doc = docNameFor(session.id);
    note = handingOffNote({
      session,
      reason,
      isMidTurn,
      successorAgent
    });

    // 1. Ask the agent to write the handoff doc, then wait for it to land.
    const hasDoc = await requestHandoffDoc({
      session,
      doc,
      reason
    });
    if (!hasDoc) {
      note = "";
      return;
    }

    // 2. End the session - once the agent has finished the handoff turn and
    // anything it started meanwhile - 3. start the successor seeded to continue.
    const isStillHereBeforeEnd = await waitForRunningWorkUnlessMidTurn({
      session,
      reason,
      isMidTurn
    });
    if (!isStillHereBeforeEnd) {
      abandonHandoff(session.id);
      return;
    }

    const { cwd } = session;
    await host.endSession(session.id);
    // Launched before the predecessor is removed, which forgets everything kept
    // for it - the successor takes over its project, model and ✦ naming first.
    const successorId = host.launchSuccessor({
      agent: successorAgent,
      cwd,
      initialPrompt: successorPrompt(
        docPathOf({
          session,
          name: doc
        })
      ),
      predecessorId: session.id
    });
    shieldHandoffSuccessor(successorId);
    host.removeSession(session.id);
    unmarkHandingOff(session.id);
    note = "";

    // 4. The doc's job ends once the successor has really used it, so consumed
    // handoffs never litter the project. Anything short of that keeps the doc:
    // losing a handoff nobody read costs the whole session's work, where a
    // leftover file costs one `rm`.
    const isConsumed = await waitForFirstTurn({
      timers,
      isStillOpen,
      id: successorId
    });
    if (isConsumed) {
      await sendMessage(Command.handoffDocDelete, {
        dir: host.projectDirectory(session),
        name: doc
      });
    }
  }

  // Ask `session`'s agent to write its handoff doc and resolve once the doc is on
  // disk - the reusable core of the handoff, WITHOUT the kill/launch. The
  // workspace-save flow uses it: the doc is written into the temp workspace, rides
  // along in the copy to the saved project, and the successor there is seeded to
  // continue from it. Returns the doc's file name, or null when the agent never
  // produced it (the caller falls back to a plain restart). A doc a prior attempt
  // already wrote is reused, not re-requested.
  async function writeHandoffDoc(session: AgentSession) {
    const doc = docNameFor(session.id);
    const isWritten = await askForDoc({
      session,
      doc,
      reason: HandoffReason.Save
    });
    return isWritten ? doc : null;
  }

  // Fire-and-forget entry point for the scan and the force path. handoff is
  // best-effort: swallow any failure (including deleting a doc the agent never
  // wrote on the timeout path) and clear the in-flight marker + note so a later
  // scan can retry.
  async function runHandoff({ session, reason, isMidTurn = false }: {
    session: AgentSession;
    reason: HandoffReason;
    isMidTurn?: boolean;
  }) {
    try {
      await handoff({
        session,
        reason,
        isMidTurn
      });
    } catch (error) {
      console.warn(`pade: handoff of ${session.agent.label} failed, a later scan retries`, error);
      abandonHandoff(session.id);
    }
  }

  // Scan for sessions to hand off: those newly over the context threshold, and any
  // whose earlier handoff was interrupted before it could reset.
  function check() {
    if (host.isOptedOut()) {
      deferredNote = "";
      return;
    }

    let heldBackNote = "";
    for (const session of host.sessions()) {
      const verdict = scanVerdict({
        session,
        isHandingOffHere: handingOff.has(session.id),
        thresholdPercentage: host.thresholdPercentage()
      });
      if (verdict === ScanVerdict.HoldBack) {
        heldBackNote ||= waitingForWorkNote({
          session,
          reason: HandoffReason.ContextLimit
        });
        continue;
      }

      if (verdict === ScanVerdict.HandOff || verdict === ScanVerdict.HandOffMidTurn) {
        markHandingOff(session.id);
        runHandoff({
          session,
          reason: HandoffReason.ContextLimit,
          isMidTurn: verdict === ScanVerdict.HandOffMidTurn
        });
      }
    }

    deferredNote = heldBackNote;
  }

  // Hand a session off right now - the usage-resume flow calls this at window
  // reset when the context is too full to just continue. Same single-flight
  // guard as the scan, none of its idle/threshold gates: the caller has
  // already decided this session must cycle. It still waits for the agent's
  // running work to finish before asking for the doc (see handoff).
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

  return {
    /** Status line shown while a handoff is in flight or held back for the agent's
     *  running work ("" when idle). */
    get note() {
      return note || deferredNote;
    },
    check,
    force,
    isHandingOff,
    restartForConfiguration,
    writeHandoffDoc,
    dispose: timers.dispose
  };
}
