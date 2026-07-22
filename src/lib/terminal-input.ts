// Terminal input shortcuts shared by every agent. A newline is pasted rather
// than typed so xterm uses bracketed-paste mode when the running TUI enables it;
// that preserves the newline in the agent's composer instead of submitting it.

export const PROMPT_NEWLINE = "\n";

export function isPromptNewlineShortcut(event: Pick<KeyboardEvent,
  "key" | "shiftKey" | "altKey" | "ctrlKey" | "metaKey"
>): boolean {
  return event.key === "Enter" && event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey;
}
