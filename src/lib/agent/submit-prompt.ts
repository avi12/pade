// Typing a request into a running agent's composer the way that survives every
// TUI: deliver it as a paste, let it settle, then submit with a SEPARATE Enter -
// re-sent until the agent is seen working. A TUI guards against the Enter that
// immediately follows a bracketed paste (so a pasted multi-line block isn't
// submitted by accident), and Codex's guard outlasts a back-to-back retry burst:
// all four rapid Enters were eaten, leaving the request unsent in the composer.
// A settled composer submits on a single Enter (verified live), so wait out the
// paste first, then re-send only the Enter, never the pasted text. An agent
// already mid-turn reads as working at once, so it gets the one Enter that
// queues the request between its steps.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { pastedText, PROMPT_SUBMIT } from "@/lib/terminal/terminal-input";
import { SessionStatus } from "@/lib/types";

const SUBMIT_SETTLE_MS = 1_200;
const SUBMIT_ATTEMPTS = 6;
const SUBMIT_CONFIRM_MS = 1_500;

/** Paste `text` into a session's composer and submit it. `wait` is the caller's
 *  delay, so one that tears down its timers on dispose can cancel the retries. */
export async function submitPastedPrompt({ id, text, wait }: {
  id: string;
  text: string;
  wait: (milliseconds: number) => Promise<unknown>;
}) {
  await sendMessage(Command.ptyWrite, {
    id,
    data: pastedText(text)
  });
  await wait(SUBMIT_SETTLE_MS);

  for (let attempt = 0; attempt < SUBMIT_ATTEMPTS; attempt += 1) {
    await sendMessage(Command.ptyWrite, {
      id,
      data: PROMPT_SUBMIT
    });
    await wait(SUBMIT_CONFIRM_MS);

    if (sessionStatus(id) === SessionStatus.enum.working) {
      return;
    }
  }
}
