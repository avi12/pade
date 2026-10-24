// What one PADE window hands another: live sessions it moves there (a
// shift-clicked session, or every session of a window merged into another
// before it closes), a recorded conversation to resume there, or a persisted
// chat to relaunch there. The backend moves the PTYs and parks the payload for
// the receiving window (`window_handoff.rs`); this module owns the payload's
// one schema - a trust boundary on arrival like any other IPC - and the pure
// rules around it: which windows a merge can go to, and what it is called.

import { SnapshotSession } from "@/lib/session/session-restore";
import { RecordedConversation } from "@/lib/types";
import type { OpenTarget, WindowInfo } from "@/lib/types";
import { shortDisplayName } from "@/lib/util/paths";
import { z } from "zod";

/** The closed set of things a window can hand another. */
export const HandoffKind = z.enum(["sessions", "conversation", "chat"]);

/** One live session on its way to another window, with its tab's name. */
const HandedSession = SnapshotSession.extend({
  project: z.string().min(1),
  label: z.string().optional()
});

const SessionsHandoff = z.object({
  kind: z.literal(HandoffKind.enum.sessions),
  sessions: z.array(HandedSession).min(1),
  /** The session to bring forward on arrival. */
  activeId: z.string().nullable()
});
export type SessionsHandoff = z.infer<typeof SessionsHandoff>;

const WindowHandoff = z.discriminatedUnion("kind", [
  SessionsHandoff,
  z.object({
    kind: z.literal(HandoffKind.enum.conversation),
    project: z.string().min(1),
    conversation: RecordedConversation
  }),
  z.object({
    kind: z.literal(HandoffKind.enum.chat),
    project: z.string().min(1),
    chatId: z.string().min(1)
  })
]);
export type WindowHandoff = z.infer<typeof WindowHandoff>;

/** The wire form of a hand-off - opaque to the backend, which only parks it. */
export function handoffPayload(handoff: WindowHandoff) {
  return JSON.stringify(WindowHandoff.parse(handoff));
}

/** The hand-offs parked for this window that parse; a malformed one is
 *  dropped with a warning rather than trusted. */
export function parseHandoffs(payloads: readonly string[]) {
  const handoffs: WindowHandoff[] = [];
  for (const payload of payloads) {
    const parsed = WindowHandoff.safeParse(parseJson(payload));
    if (parsed.success) {
      handoffs.push(parsed.data);
    } else {
      console.warn("pade: dropped a malformed window hand-off", parsed.error);
    }
  }

  return handoffs;
}

/** The payload's JSON, or null for text that is not JSON - which the schema
 *  then refuses like any other malformed hand-off. */
function parseJson(payload: string) {
  try {
    const value: unknown = JSON.parse(payload);
    return value;
  } catch {
    return null;
  }
}

/** The project a hand-off lands in - the window boots into it. */
export function handoffProject(handoff: WindowHandoff) {
  if (handoff.kind === HandoffKind.enum.sessions) {
    const front = handoff.sessions.find(session => session.id === handoff.activeId) ?? handoff.sessions[0];
    return front.project;
  }

  return handoff.project;
}

/** How the receiving window enters a handed conversation or chat: beside
 *  whatever it already runs, the same way a rail click enters it. */
export function joinTargetOf(handoff: Exclude<WindowHandoff, SessionsHandoff>): OpenTarget {
  if (handoff.kind === HandoffKind.enum.conversation) {
    return {
      path: handoff.project,
      resume: handoff.conversation
    };
  }

  return {
    path: handoff.project,
    chatId: handoff.chatId
  };
}

/** What a merge action says it does, wherever it is offered. */
export const MERGE_INTO_TOOLTIP = "Move this window's sessions there and close this one";

/** The windows this one can merge into: every other open window. */
export function mergeTargets(windowRows: readonly WindowInfo[]) {
  return windowRows.filter(windowRow => !windowRow.isCurrent);
}

/** The menu item below the open-windows list that merges this window into the
 *  only other one - none when there is no other, or several (each of their
 *  rows carries its own merge action then). */
export function mergeMenuItemLabel({ windowRows, labels }: {
  windowRows: readonly WindowInfo[];
  labels: Record<string, string>;
}) {
  const targets = mergeTargets(windowRows);
  if (targets.length !== 1) {
    return null;
  }

  return `Merge into ${shortDisplayName({
    path: targets[0].path,
    labels
  })}`;
}
