// What the user has going in each agent's composer, so PADE never types its
// own prompt (a handoff request, pacing's "continue") over theirs: one landed
// at the cursor mid-sentence and was submitted fused with the user's words.

import { registerSessionState, SessionStateKey } from "@/lib/session/lifecycle/session-scope";
import { nextDraftLength } from "@/lib/terminal/input/composer-draft";

/** A keystroke this recent means the user is mid-thought, draft or not. */
const TYPING_QUIET_MS = 5_000;
/** A draft untouched this long is trusted no more: the estimate may have missed
 *  how it was cleared (Ctrl+Backspace, a mouse selection), and holding PADE's
 *  prompt back for good would be worse than the rare collision. */
const DRAFT_TRUST_MS = 5 * 60_000;
/** How often a waiting prompt checks the composer again. */
const COMPOSER_POLL_MS = 1_000;

const drafts = new Map<
  string,
  {
    length: number;
    lastKeystrokeAt: number;
  }
>();

registerSessionState({
  key: SessionStateKey.enum.composerDraft,
  state: {
    forget(id) {
      drafts.delete(id);
    }
  }
});

/** The user sent `data` to a session from its pane. */
export function noteComposerInput({ id, data }: { id: string; data: string }) {
  drafts.set(id, {
    length: nextDraftLength({
      length: drafts.get(id)?.length ?? 0,
      data
    }),
    lastKeystrokeAt: Date.now()
  });
}

/** Whether the user is typing in the session's composer, or left a draft in it. */
export function isComposing(id: string) {
  const draft = drafts.get(id);
  if (draft === undefined) {
    return false;
  }

  const quietFor = Date.now() - draft.lastKeystrokeAt;
  const isTyping = quietFor < TYPING_QUIET_MS;
  const hasTrustedDraft = draft.length > 0 && quietFor < DRAFT_TRUST_MS;
  return isTyping || hasTrustedDraft;
}

/** Resolve once the session's composer is the user's no more - they stopped
 *  typing and sent or cleared what they wrote. `wait` is the caller's delay, so
 *  one that tears down its timers on dispose can cancel the wait. */
export async function whenComposerFree({ id, wait }: { id: string; wait: (milliseconds: number) => Promise<unknown> }) {
  while (isComposing(id)) {
    await wait(COMPOSER_POLL_MS);
  }
}
