// Getting the handoff doc written: asking the agent for it, waiting for it to
// land on disk, retrying a bounded number of times and, once out of attempts,
// watching for it to turn up late. Also the gate that holds a handoff while the
// agent still has work running.

import { submitPastedPrompt } from "@/lib/agent/submit-prompt";
import { agentReportsBusy, whenAgentSettled } from "@/lib/stores/agent/agentActivity.svelte";
import { isFileOnDisk, waitForFile } from "@/lib/stores/agent/handoff/handoff-doc-wait";
import type { HandoffHost } from "@/lib/stores/agent/handoff/handoff-host";
import { waitingForWorkNote } from "@/lib/stores/agent/handoff/handoff-notes";
import { handoffRequestBody } from "@/lib/stores/agent/handoff/handoff-prompts";
import type { HandoffReason } from "@/lib/stores/agent/handoff/handoff-prompts";
import type { HandoffState } from "@/lib/stores/agent/handoff/handoff-state.svelte";
import type { createHandoffTimers } from "@/lib/stores/agent/handoff/handoff-timers";
import { takeOverPacing } from "@/lib/stores/usage/usagePacing.svelte";
import type { AgentSession } from "@/lib/types/agent";

// How many times a session may be re-asked for its handoff doc. Each retry
// only happens once the session reads ready again, so this bounds the
// prompt-noise for an agent that answers without ever writing the file;
// after the last attempt the session stays marked and keeps running.
const HANDOFF_ATTEMPT_LIMIT = 3;
/** How often a session out of attempts is checked for a doc that turned up late. */
const LATE_DOC_POLL_MS = 15_000;

export interface HandoffContext {
  host: HandoffHost;
  state: HandoffState;
  timers: ReturnType<typeof createHandoffTimers>;
  /** Run the whole handoff - from a trigger, or again once a late doc arrives. */
  runHandoff: (request: {
    session: AgentSession;
    reason: HandoffReason;
    isMidTurn?: boolean;
  }) => Promise<void>;
}

export function isStillOpen({ context, sessionId }: {
  context: HandoffContext;
  sessionId: string;
}) {
  return context.host.sessions().some(candidate => candidate.id === sessionId);
}

export function docPathOf({ context, session, name }: {
  context: HandoffContext;
  session: AgentSession;
  name: string;
}) {
  return `${context.host.projectDirectory(session)}/${name}`;
}

async function submitHandoffRequest({ context, session, doc, reason }: {
  context: HandoffContext;
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
        context,
        session,
        name: doc
      }),
      reason
    }),
    wait: context.timers.afterDelay
  });
}

// Hold the handoff until the agent reports no work in flight, so ending the
// session never severs a workflow or background agent it is still running.
// Resolves whether the session is still here to hand off.
async function waitForRunningWork({ context, session, reason }: {
  context: HandoffContext;
  session: AgentSession;
  reason: HandoffReason;
}) {
  if (agentReportsBusy(session.id)) {
    context.state.note = waitingForWorkNote({
      session,
      reason
    });
    await whenAgentSettled(session.id);
  }

  return isStillOpen({
    context,
    sessionId: session.id
  });
}

// The same gate, skipped when the handoff has to beat the agent's own
// compaction: then the running turn is exactly what it must not wait for.
export async function waitForRunningWorkUnlessMidTurn({ context, session, reason, isMidTurn }: {
  context: HandoffContext;
  session: AgentSession;
  reason: HandoffReason;
  isMidTurn: boolean;
}) {
  if (isMidTurn) {
    return isStillOpen({
      context,
      sessionId: session.id
    });
  }

  return waitForRunningWork({
    context,
    session,
    reason
  });
}

// Ask for the handoff doc and resolve whether it is on disk - unless a
// previous attempt already produced it: a request sent to a busy agent gets
// queued and answered long after the wait below expired, so a retry must
// consume the doc that late answer wrote instead of asking for a second one.
export async function askForDoc({ context, session, doc, reason }: {
  context: HandoffContext;
  session: AgentSession;
  doc: string;
  reason: HandoffReason;
}) {
  const docPath = docPathOf({
    context,
    session,
    name: doc
  });
  if (await isFileOnDisk(docPath)) {
    return true;
  }

  await submitHandoffRequest({
    context,
    session,
    doc,
    reason
  });

  // The watcher can simply have missed the write (an ignored path, a
  // torn-down subscription), so a timeout gets one direct disk probe before
  // giving up.
  return await waitForFile({
    timers: context.timers,
    isStillOpen: sessionId => isStillOpen({
      context,
      sessionId
    }),
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
export async function requestHandoffDoc({ context, session, doc, reason }: {
  context: HandoffContext;
  session: AgentSession;
  doc: string;
  reason: HandoffReason;
}) {
  const { state } = context;
  if (await askForDoc({
    context,
    session,
    doc,
    reason
  })) {
    return true;
  }

  if (!isStillOpen({
    context,
    sessionId: session.id
  })) {
    state.abandonHandoff(session.id);
    return false;
  }

  const attempts = (state.handoffAttempts.get(session.id) ?? 0) + 1;
  state.handoffAttempts.set(session.id, attempts);

  if (attempts < HANDOFF_ATTEMPT_LIMIT) {
    state.unmarkHandingOff(session.id);
  } else {
    watchForLateDoc({
      context,
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
function watchForLateDoc({ context, session, doc, reason }: {
  context: HandoffContext;
  session: AgentSession;
  doc: string;
  reason: HandoffReason;
}) {
  const { timers, state } = context;
  const docPath = docPathOf({
    context,
    session,
    name: doc
  });
  async function look() {
    if (!isStillOpen({
      context,
      sessionId: session.id
    })) {
      return;
    }

    if (!await isFileOnDisk(docPath)) {
      timers.trackTimer({
        handler: look,
        delayMs: LATE_DOC_POLL_MS
      });
      return;
    }

    state.handoffAttempts.delete(session.id);
    await context.runHandoff({
      session,
      reason
    });
  }

  timers.trackTimer({
    handler: look,
    delayMs: LATE_DOC_POLL_MS
  });
}
