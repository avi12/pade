// Best-effort detection of a known task's command inside an agent's terminal
// output — the signal that lets the Tasks panel reflect a task the agent started
// as "running" (see stores/taskRuns). This must be evidence of an execution, not
// merely an agent mentioning a command in its summary. The PTY carries ANSI colour
// codes and agents render tool calls as `PowerShell(command)` / `Bash(command)`,
// while a visible shell invocation begins at a prompt. Strip ANSI, then recognise
// only those two concrete forms.

import { stripAnsi } from "@/lib/ansi";

const TOOL_NAMES = "Bash|PowerShell|Shell|Terminal";

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Whether `line` proves that `command` was invoked: a shell-prompt line or an
 *  agent tool-call rendering. Plain prose such as "verified with pnpm lint" is
 *  intentionally not enough to set a task's running state. */
export function isTaskInvocation({ line, command }: {
  line: string;
  command: string;
}): boolean {
  const clean = stripAnsi(line).trim();
  const escapedCommand = escapeRegExp(command);
  const shellPrompt = new RegExp(`^[$#%❯>]\\s*${escapedCommand}(?:\\s|$)`);
  const toolCall = new RegExp(
    `(?:^|\\s)(?:${TOOL_NAMES})\\(\\s*${escapedCommand}\\s*\\)(?:\\s|$)`
  );
  return shellPrompt.test(clean) || toolCall.test(clean);
}
