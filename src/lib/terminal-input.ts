// Terminal input shortcuts shared by every agent. A newline is pasted rather
// than typed so xterm uses bracketed-paste mode when the running TUI enables it;
// that preserves the newline in the agent's composer instead of submitting it.

export const PROMPT_NEWLINE = "\n";

// Bracketed-paste delivery for a whole programmatic prompt (the first prompt,
// the handoff request): the paste markers make the TUI hold the text in its
// composer, and the ENTER appended AFTER the closing marker is an unambiguous,
// separate keystroke that submits it. Raw bytes with a trailing CR instead let
// the agent fold the CR into the same burst and leave the prompt sitting
// unsent. Wire constants composed from named parts (no obfuscated values).
const CONTROL_SEQUENCE_INTRODUCER = "\x1b[";
const BRACKETED_PASTE_START_PARAMETER = "200";
const BRACKETED_PASTE_END_PARAMETER = "201";
const TILDE_FINAL_BYTE = "~";
const ENTER = "\r";

export const BRACKETED_PASTE_START =
  `${CONTROL_SEQUENCE_INTRODUCER}${BRACKETED_PASTE_START_PARAMETER}${TILDE_FINAL_BYTE}`;
export const BRACKETED_PASTE_END =
  `${CONTROL_SEQUENCE_INTRODUCER}${BRACKETED_PASTE_END_PARAMETER}${TILDE_FINAL_BYTE}`;

/** A prompt wrapped for paste-then-submit delivery to a TUI's composer. */
export function submittedPrompt(prompt: string): string {
  return `${BRACKETED_PASTE_START}${prompt}${BRACKETED_PASTE_END}${ENTER}`;
}

export function isPromptNewlineShortcut(event: Pick<KeyboardEvent,
  "key" | "shiftKey" | "altKey" | "ctrlKey" | "metaKey"
>): boolean {
  return event.key === "Enter" && event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey;
}
