// Recovery for an in-place MCP server-definition edit. Membership changes are
// handed off immediately by App; a same-name edit is left for the user/agent's
// normal reload path. Only if that pending session then reports an MCP reload
// failure do we ask App to intervene with the safe document handoff.

import { stripAnsi } from "@/lib/ansi";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

const FAILURE_RE = /(?:\bmcp\b[\s\S]{0,200}\b(?:error|fail(?:ed|ure)?|invalid|unable)\b|\b(?:error|fail(?:ed|ure)?|invalid|unable)\b[\s\S]{0,200}\bmcp\b)/i;
const SUCCESS_RE = /\bmcp\b[\s\S]{0,200}\b(?:reloaded|reload succeeded)\b/i;
const OUTPUT_TAIL_LENGTH = 1_000;

const pending = new SvelteMap<string, string>();
const failed = new SvelteSet<string>();

/** Remember sessions whose same-name MCP definitions changed. */
export function armMcpReloadRecovery(sessionIds: readonly string[]): void {
  for (const id of sessionIds) {
    pending.set(id, "");
    failed.delete(id);
  }
}

/** Whether terminal output reports a failed MCP reload. Exported for focused
 * tests; the live observer additionally requires a pending config edit. */
export function parseMcpReloadFailure({ text }: { text: string }): boolean {
  return FAILURE_RE.test(stripAnsi(text));
}

/** Feed one PTY chunk into the bounded detector for a pending session. */
export function observeMcpReload({ id, chunk }: {
  id: string;
  chunk: string;
}): void {
  const previous = pending.get(id);
  if (previous === undefined || failed.has(id)) {
    return;
  }

  const output = `${previous}${stripAnsi(chunk)}`.slice(-OUTPUT_TAIL_LENGTH);
  if (FAILURE_RE.test(output)) {
    failed.add(id);
    pending.delete(id);
    return;
  }

  if (SUCCESS_RE.test(output)) {
    pending.delete(id);
    return;
  }

  pending.set(id, output);
}

/** Reactive ids whose manual/agent reload failed and need intervention. */
export function failedMcpReloads(): string[] {
  return [...failed];
}

/** Clear all reload state when recovery starts or a session ends. */
export function dropMcpReload(id: string): void {
  pending.delete(id);
  failed.delete(id);
}
