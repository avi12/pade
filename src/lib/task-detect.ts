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

/** Whether `command` appears as a whole shell token run inside `text` — bounded on
 *  both sides by a command separator (start/end, whitespace, `;`, `&`, `|`, a
 *  paren, or a redirect). This is what lets `pnpm dev` match inside the way an
 *  agent actually runs it — `cd app && pnpm dev 2>&1` — while never matching a
 *  prefix of a longer command like `pnpm dev:prod`. */
function containsCommand({ text, command }: {
  text: string;
  command: string;
}): boolean {
  const bounded = new RegExp(
    `(?:^|[\\s;&|(])${escapeRegExp(command)}(?=$|[\\s;&|)<>])`
  );
  return bounded.test(text);
}

/** Whether `line` proves that `command` was invoked: a shell-prompt line or an
 *  agent tool-call rendering. Plain prose such as "verified with pnpm lint" is
 *  intentionally not enough to set a task's running state. Agents wrap the real
 *  command with a `cd … &&` prefix, env assignments, arguments and redirects, so
 *  the command is matched as a bounded token WITHIN the invocation rather than as
 *  its entire text. */
export function isTaskInvocation({ line, command }: {
  line: string;
  command: string;
}): boolean {
  const clean = stripAnsi(line).trim();

  // Agent tool-call rendering: Bash(<command>) / PowerShell(<command>). Look for
  // the command bounded inside the parentheses' contents.
  const toolCall = new RegExp(`(?:^|\\s)(?:${TOOL_NAMES})\\(([^)]*)\\)`);
  const toolMatch = toolCall.exec(clean);
  if (toolMatch && containsCommand({
    text: toolMatch[1],
    command
  })) {
    return true;
  }

  // Visible shell invocation: a prompt, then whatever the user typed after it.
  const promptMatch = /^[$#%❯>]\s*(.*)$/.exec(clean);
  return promptMatch !== null && containsCommand({
    text: promptMatch[1],
    command
  });
}
