import { submittedPrompt } from "@/lib/terminal/input/terminal-input";

/** The doc-name token for a session id with no usable leading segment, and the
 *  slug for a source with no usable characters. */
const FALLBACK_NAME_PART = "session";

export const HandoffReason = {
  CONTEXT_LIMIT: "context-limit",
  CONFIGURATION_CHANGE: "configuration-change",
  SAVE: "save"
} as const;
export type HandoffReason = (typeof HandoffReason)[keyof typeof HandoffReason];

/** The one sentence that opens the handoff request, per reason - the SSOT so the
 *  wording never drifts and adding a reason is one entry, not a new branch. */
const HANDOFF_TRIGGER: Record<HandoffReason, string> = {
  [HandoffReason.CONTEXT_LIMIT]: "Your context window is nearly full.",
  [HandoffReason.CONFIGURATION_CHANGE]:
    "The project's MCP server configuration changed, so PADE must restart this agent to load it.",
  [HandoffReason.SAVE]:
    "This workspace is being saved as a permanent project, and its agent is restarting in the saved location."
};

/** A filesystem-safe slug for the handoff doc, from the workspace label/dir. */
export function handoffSlug(source: string) {
  const slug = source
    .replaceAll(/[^a-z0-9-]+/gi, "-")
    .replaceAll(/^-+|-+$/g, "")
    .toLowerCase();
  return slug || FALLBACK_NAME_PART;
}

/** The leading segment of a session's UUID - a short, stable per-session token
 *  that keeps the handoff doc name unique without dragging the whole id into a
 *  filename. Falls back to a generic token for a non-UUID id. */
function sessionToken(sessionId: string) {
  return sessionId.split("-")[0] || FALLBACK_NAME_PART;
}

/** The handoff doc's file name for one session. Unique PER SESSION, not just per
 *  project: two agents open in the SAME project must never share one
 *  `continue-<project>.md`. When they did, one session's doc-write fired the
 *  other session's `waitForFile` (it matches on file name alone), tearing the
 *  wrong - usually idle - session down; the project slug keeps the name
 *  human-readable, the session token keeps it collision-free. */
export function handoffDocName({ source, sessionId }: { source: string; sessionId: string }) {
  return `continue-${handoffSlug(source)}-${sessionToken(sessionId)}.md`;
}

/** The handoff request's text, without any paste framing or submit keystroke -
 *  what the agent is asked to do. The store pastes this and submits it with a
 *  separate, retried Enter (see `submitHandoffRequest`). */
export function handoffRequestBody({ doc, reason }: { doc: string; reason: HandoffReason }) {
  return `${HANDOFF_TRIGGER[reason]} Please write a concise handoff to ${doc} - the current state, what you've completed, and the exact next steps to continue - then stop.`;
}

export function handoffPrompt({ doc, reason }: { doc: string; reason: HandoffReason }) {
  return submittedPrompt(
    handoffRequestBody({
      doc,
      reason
    })
  );
}

/** Seed for the fresh successor: read ONLY the handoff doc and continue. The
 *  whole point of a handoff is a small, clean context - so it deliberately does
 *  NOT ask the successor to also read CLAUDE.md or earlier continue-*.md files.
 *  The agent auto-loads its project memory (CLAUDE.md) on its own, and re-reading
 *  stale handoffs would bloat the very context the handoff exists to reset. The
 *  doc must therefore be self-sufficient: current state + exact next steps.
 *  Agent-agnostic - it names a file on disk, not any one agent's memory system.
 *
 *  No trailing carriage return: this rides in as the successor session's
 *  initialPrompt, and the terminal's initial-prompt delivery appends the
 *  submitting ENTER itself (see panels/Terminal.svelte, lib/agent/initial-prompt). */
export function successorPrompt(doc: string) {
  return `Read ${doc} to continue the work where the previous session left off.`;
}
