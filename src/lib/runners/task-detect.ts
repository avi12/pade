// Best-effort detection of a known task's command inside an agent's terminal
// output - the signal that lets the Tasks panel reflect a task the agent started
// as "running" (see stores/taskRuns). This must be evidence of an execution, not
// merely an agent mentioning a command in its summary. The PTY carries ANSI colour
// codes and agents render tool calls as `PowerShell(command)` / `Bash(command)`,
// while a visible shell invocation begins at a prompt. Strip ANSI, then recognise
// only those two concrete forms.

import { stripAnsi } from "@/lib/terminal/output/ansi";

const TOOL_NAMES = "Bash|PowerShell|Shell|Terminal";
const REGEX_SPECIAL_CHARACTER = /[.*+?^${}()|[\]\\]/g;

/** An agent's tool-call rendering - `Bash(<command>)` / `PowerShell(<command>)` -
 *  capturing the parentheses' contents. */
const TOOL_CALL = new RegExp(`(?:^|\\s)(?:${TOOL_NAMES})\\(([^)]*)\\)`);

/** A visible shell invocation: a prompt, then whatever was typed after it. */
const PROMPT_LINE = /^[$#%❯>]\s*(.*)$/;

function escapeRegExp(value: string) {
  return value.replaceAll(REGEX_SPECIAL_CHARACTER, "\\$&");
}

/** Whether `command` appears as a whole shell token run inside `text` - bounded on
 *  both sides by a command separator (start/end, whitespace, `;`, `&`, `|`, a
 *  paren, or a redirect). This is what lets `pnpm dev` match inside the way an
 *  agent actually runs it - `cd app && pnpm dev 2>&1` - while never matching a
 *  prefix of a longer command like `pnpm dev:prod`. */
function containsCommand({ text, command }: {
  text: string;
  command: string;
}) {
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
}) {
  const clean = stripAnsi(line).trim();
  const invocations = [TOOL_CALL.exec(clean)?.[1], PROMPT_LINE.exec(clean)?.[1]];
  return invocations.some(text => text !== undefined && containsCommand({
    text,
    command
  }));
}
