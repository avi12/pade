// Best-effort detection of a known task's command inside an agent's terminal
// output — the signal that lets the Tasks panel reflect a task the agent started
// as "running" (see stores/taskRuns). Two things make a naive match miss it: the
// PTY carries ANSI colour codes, and the agent renders a tool call as
// `Tool(command)` (`PowerShell(pnpm dev)`, `Bash(pnpm build)`), so the command
// sits wrapped in parentheses rather than sitting at a shell prompt. Strip the
// ANSI, then treat the `(`/`)` wrapper as a word boundary alongside whitespace
// and the usual prompt sigils.

import { stripAnsi } from "@/lib/ansi";

// Characters that may legitimately abut a command invocation: whitespace, a
// shell prompt sigil, or the opening paren an agent wraps a tool call's command
// in (its close paren is the matching after-boundary).
const BOUNDARY_BEFORE = /[\s$>#%❯(]/;
const BOUNDARY_AFTER = /[\s)]/;

/** Whether `command` appears as a whole invocation somewhere in `line`, bounded
 *  on both sides so `pnpm build` doesn't match `pnpm build:prod`, and recognising
 *  the `Tool(command)` form an agent renders its tool calls in (through the ANSI
 *  colour codes the transcript is painted with). */
export function isTaskInvocation({ line, command }: {
  line: string;
  command: string;
}): boolean {
  const clean = stripAnsi(line).trim();
  const index = clean.indexOf(command);
  if (index < 0) {
    return false;
  }

  const before = index === 0 ? "" : clean[index - 1];
  const afterIndex = index + command.length;
  const after = afterIndex >= clean.length ? "" : clean[afterIndex];
  const boundaryBefore = before === "" || BOUNDARY_BEFORE.test(before);
  const boundaryAfter = after === "" || BOUNDARY_AFTER.test(after);
  return boundaryBefore && boundaryAfter;
}
