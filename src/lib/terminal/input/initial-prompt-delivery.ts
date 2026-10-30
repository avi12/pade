// Initial-prompt delivery (see lib/agent/initial-prompt for the screen reading).
// A fresh agent gates on a "trust this folder?" prompt before its input line is
// live, so the first prompt can't just be fired on mount - it would land in the
// menu or sit unsent. Instead the pane watches the rendered screen: walk the
// trust gate's selection to "Yes, I trust this folder" and confirm it, then once
// the agent has settled quiet at its REPL, paste the prompt and submit it.

import { promptEchoed, TrustGateKey, trustGateKey } from "@/lib/agent/initial-prompt";
import { SUBMIT_VERIFY_MS } from "@/lib/terminal/activity/activity-timing";
import { pastedText } from "@/lib/terminal/input/terminal-input";
import { CURSOR_DOWN, CURSOR_UP, ENTER } from "@/lib/terminal/input/terminal-sequences";
import { afterDelay } from "@/lib/util/async/delay";

// Keys sent to walk and confirm the gate, bounded so a gate whose layout we
// misread can't be keyed at forever - the user answers it by hand instead.
const TRUST_GATE_MAXIMUM_KEYS = 6;
// A rolling tail of recent output, so delivery can tell whether the prompt's
// paste has echoed back in the composer.
const RECENT_OUTPUT_CAP = 8_000;

const TRUST_GATE_KEY_BYTES = {
  [TrustGateKey.Confirm]: ENTER,
  [TrustGateKey.Down]: CURSOR_DOWN,
  [TrustGateKey.Up]: CURSOR_UP
} as const satisfies Record<TrustGateKey, string>;

// A freshly-spawned TUI can paint its whole splash while not yet READING
// stdin (OpenCode/Claude in a brand-new workspace do first-run setup after
// first paint), so a prompt written the moment the output settles can be
// silently swallowed. Delivery is therefore verified: after each attempt the
// output is watched for the prompt echoing back in the composer, and until it
// does the attempt repeats on a short timer - a deliberate poll, since no
// event announces "the TUI now reads input". Bounded so an agent that renders
// the prompt unrecognizably can't be pasted at forever. Each attempt is checked
// `SUBMIT_VERIFY_MS` after it, past the echo of its paste.
// Let a fresh bracketed paste land in the composer before the submitting Enter,
// so the TUI's post-paste guard doesn't fold the CR into the paste burst and
// leave the prompt sitting unsent (a combined paste+CR is exactly what stranded
// a handoff successor's prompt, typed but never submitted).
const PROMPT_SUBMIT_SETTLE_MS = 1_200;
const PROMPT_MAXIMUM_ATTEMPTS = 8;

export function createInitialPromptDelivery({ prompt, write, isWorking, settle }: {
  /** The session's first prompt, while it still waits to be delivered. */
  prompt: () => string | undefined;
  write: (data: string) => Promise<void>;
  /** The session is busy right now (not settled at its prompt). */
  isWorking: () => boolean;
  /** Delivery is over - the agent took the prompt up, or it was given up on -
   *  so the prompt stops being pending wherever the session goes next. */
  settle: () => void;
}) {
  let isDelivered = false;

  function finishDelivery() {
    isDelivered = true;
    settle();
  }
  // Whether the last screen sample showed the trust gate. Delivery holds while it
  // does - a prompt pasted into the gate's menu is lost.
  let isTrustGateOnScreen = false;
  let trustGateKeysSent = 0;
  // A key was sent and the agent hasn't repainted since, so the screen still
  // shows the gate as it was BEFORE the key - acting on it would double-step.
  let isTrustGateAwaitingRepaint = false;
  let recentOutput = "";
  let attempts = 0;
  // A submit is written and waiting for its verdict. The idle handler fires its
  // own re-check once the idle window passes - before the verdict - so it must not
  // re-enter delivery meanwhile, or it would re-paste (or prematurely latch)
  // while the verify timer is still deciding. The verify callback owns the retry.
  let isAwaitingVerify = false;
  let verifyTimer: ReturnType<typeof setTimeout> | undefined;

  function isPending() {
    return Boolean(prompt()) && !isDelivered;
  }

  // Once the agent has settled quiet - done booting, past the trust gate - paste
  // the first prompt, then submit it and re-attempt until the agent is seen
  // working. The bracketed paste keeps the prompt's own newlines soft; the
  // submitting ENTER is sent separately (a raw write folds the CR into the paste
  // and leaves the prompt unsent) and re-sent on its own if the TUI's post-paste
  // guard swallowed the first one.
  async function deliverIfReady() {
    const text = prompt();
    if (!text || isDelivered || isAwaitingVerify) {
      return;
    }

    // The gate is still up - answerTrustGate (or the user) clears it, and the
    // agent's repaint settles back to ready, which re-runs this.
    if (isTrustGateOnScreen) {
      return;
    }

    if (attempts >= PROMPT_MAXIMUM_ATTEMPTS) {
      finishDelivery();
      clearTimeout(verifyTimer);
      return;
    }

    // The prompt echoing back means the PASTE reached the composer - NOT that it
    // was submitted. A TUI's post-paste guard (Claude Code's especially) swallows
    // the ENTER that rides in the same burst as the bracketed paste, leaving the
    // text sitting unsent; latching on the echo alone is what stranded a handoff
    // successor with its prompt typed but never sent, so the user had to press
    // Enter by hand. A prompt already in the composer is therefore submitted with a
    // SEPARATE ENTER - a standalone keystroke the guard can't fold into the paste -
    // re-sent until the agent is seen working; only a prompt that never echoed is
    // (re)pasted. `working` past the paste-echo settle (the verify below) is the
    // one signal it truly went in.
    const isPasteLanded = promptEchoed({
      output: recentOutput,
      prompt: text
    });
    attempts += 1;
    isAwaitingVerify = true;

    if (isPasteLanded) {
      // The paste already reached the composer; only its submitting Enter was
      // swallowed, so re-send that alone - never re-paste over the standing text.
      await write(ENTER);
    } else {
      // Fresh delivery: paste, let the composer SETTLE, then submit with a
      // SEPARATE Enter. A one-shot paste+CR rides a single burst and the TUI's
      // post-paste guard eats the CR; the settle is what makes the Enter land on
      // this first attempt instead of leaning on the retry/verify to recover.
      await write(pastedText(text));
      await afterDelay(PROMPT_SUBMIT_SETTLE_MS);
      await write(ENTER);
    }

    // Verify shortly. Still `working` at the verdict means the agent took the prompt
    // up and is running it - delivery is done, so latch it (this is what stops a
    // handoff successor, whose first turn scrolls the echo out of `recentOutput`,
    // from being re-prompted). Back at `ready` means it wasn't submitted - the
    // bundled Enter was swallowed, or the TUI wasn't reading yet - so try again
    // (a separate Enter when the paste already landed, else a fresh paste).
    clearTimeout(verifyTimer);
    verifyTimer = setTimeout(() => {
      isAwaitingVerify = false;

      if (isWorking()) {
        finishDelivery();
        return;
      }

      deliverIfReady();
    }, SUBMIT_VERIFY_MS);
  }

  return {
    deliverIfReady,
    /** Keep the tail of a fresh session's output while its first prompt is
     *  pending, for delivery's echo check; any output also means the agent has
     *  repainted since the last trust-gate key. */
    watchOutput(data: string) {
      if (!isPending()) {
        return;
      }

      recentOutput = (recentOutput + data).slice(-RECENT_OUTPUT_CAP);
      isTrustGateAwaitingRepaint = false;
    },
    /** Answer the first-run trust gate from the rendered screen: step its
     *  selection onto "Yes, I trust this folder", then confirm - so the prompt
     *  delivered next lands at the REPL and not in the menu. Runs only while a
     *  first prompt is still pending; one key per repaint. */
    async answerTrustGate(rows: readonly string[]) {
      if (!isPending()) {
        return;
      }

      const key = trustGateKey(rows);
      isTrustGateOnScreen = key !== null;
      const mayPressKey = isTrustGateOnScreen && !isTrustGateAwaitingRepaint
        && trustGateKeysSent < TRUST_GATE_MAXIMUM_KEYS;
      if (!key || !mayPressKey) {
        return;
      }

      trustGateKeysSent += 1;
      isTrustGateAwaitingRepaint = true;
      await write(TRUST_GATE_KEY_BYTES[key]);
    },
    /** The session ended or the pane is going: drop a pending verdict. */
    cancel() {
      clearTimeout(verifyTimer);
    }
  };
}
