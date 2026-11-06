// The keys each agent answers to when PADE must interrupt it through its
// terminal: its interrupt (end the running turn) and, where it has one, its
// stop-all-agents action (end every background subagent, which the interrupt
// leaves running). Usage pacing presses them when a stop can't reach a session
// through its hook (lib/stores/usage/pacingInterrupt).
//
// One table, each entry from the agent's own build, never guessed - an action
// nobody verified is `null` (unsupported):
//
//   * Claude Code 2.1.302: Esc is `chat:cancel`; `chat:killAgents` is
//     ctrl+x ctrl+k, pressed twice within 3 s (the first press only asks
//     "again to stop background agents"). Both live in the Chat context and
//     the user may rebind them in ~/.claude/keybindings.json. Proven on a real
//     background subagent: the chord stopped it, Esc alone does not.
//   * OpenCode: `session_interrupt` is Esc, pressed twice within 5 s; it has
//     no stop-all action.
//   * Codex: Esc interrupts the turn; no stop-all action is known.
//   * Copilot: Esc cancels; no stop-all action is known.

import { AgentId } from "@/lib/agent/agent-icon";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { encodeChord, normalizeChord } from "@/lib/terminal/input/key-chord";
import type { ClaudeKeybindings } from "@/lib/types";

/** One shortcut: its default chord, how many presses it takes, and the action
 *  a user may rebind it under (null where the agent reads no user keys). */
interface AgentShortcut {
  chord: string;
  presses: number;
  binding: {
    action: string;
    context: string;
  } | null;
}

interface AgentInterruptShortcuts {
  interrupt: AgentShortcut;
  stopAll: AgentShortcut | null;
  /** Whether the agent can take pacing's stop mid-turn on its own (a hook or
   *  plugin refusing its tool calls) - else the terminal is the only way. */
  hasMidTurnStopHook: boolean;
}

const ESCAPE_CHORD = "escape";
const CLAUDE_CHAT_CONTEXT = "Chat";

const ESCAPE_ONCE: AgentShortcut = {
  chord: ESCAPE_CHORD,
  presses: 1,
  binding: null
};

const AGENT_INTERRUPT_SHORTCUTS: Readonly<Partial<Record<string, AgentInterruptShortcuts>>> = {
  [AgentId.Claude]: {
    interrupt: {
      chord: ESCAPE_CHORD,
      presses: 1,
      binding: {
        action: "chat:cancel",
        context: CLAUDE_CHAT_CONTEXT
      }
    },
    stopAll: {
      chord: "ctrl+x ctrl+k",
      presses: 2,
      binding: {
        action: "chat:killAgents",
        context: CLAUDE_CHAT_CONTEXT
      }
    },
    hasMidTurnStopHook: true
  },
  [AgentId.Opencode]: {
    interrupt: {
      ...ESCAPE_ONCE,
      presses: 2
    },
    stopAll: null,
    hasMidTurnStopHook: true
  },
  [AgentId.Codex]: {
    interrupt: ESCAPE_ONCE,
    stopAll: null,
    hasMidTurnStopHook: false
  },
  [AgentId.Copilot]: {
    interrupt: ESCAPE_ONCE,
    stopAll: null,
    hasMidTurnStopHook: false
  }
};

/** The chord a shortcut is pressed with under the user's keybindings: the
 *  default while it still means this action (a user binding adds to the
 *  defaults), else the user's last chord for it, else none. Pure. */
function resolveChord({ shortcut, keybindings }: {
  shortcut: AgentShortcut;
  keybindings: ClaudeKeybindings | null;
}) {
  const binding = shortcut.binding;
  if (binding === null || keybindings === null) {
    return shortcut.chord;
  }

  const defaultChord = normalizeChord(shortcut.chord);
  let isDefaultKept = true;
  let userChord: string | null = null;
  for (const block of keybindings.bindings) {
    if (block.context !== binding.context) {
      continue;
    }

    for (const [chord, action] of Object.entries(block.bindings)) {
      const isThisAction = action === binding.action;
      if (normalizeChord(chord) === defaultChord) {
        isDefaultKept = isThisAction;
      }

      if (isThisAction) {
        userChord = chord;
      }
    }
  }

  return isDefaultKept ? shortcut.chord : userChord;
}

function keyPress({ shortcut, keybindings }: {
  shortcut: AgentShortcut | null;
  keybindings: ClaudeKeybindings | null;
}) {
  if (shortcut === null) {
    return null;
  }

  const chord = resolveChord({
    shortcut,
    keybindings
  });
  const keystrokes = chord === null ? null : encodeChord(chord);
  return keystrokes === null ? null : {
    keystrokes,
    presses: shortcut.presses
  };
}

/** `command`'s interrupt keys under `keybindings`, or null for an agent PADE
 *  knows none for. Pure. */
export function interruptKeysOf({ command, keybindings }: {
  command: string;
  keybindings: ClaudeKeybindings | null;
}) {
  const shortcuts = AGENT_INTERRUPT_SHORTCUTS[command];
  if (shortcuts === undefined) {
    return null;
  }

  return {
    interrupt: keyPress({
      shortcut: shortcuts.interrupt,
      keybindings
    }),
    stopAll: keyPress({
      shortcut: shortcuts.stopAll,
      keybindings
    }),
    hasMidTurnStopHook: shortcuts.hasMidTurnStopHook
  };
}

/** What an agent's interrupt and stop-all are pressed with, after the user's
 *  own keybindings - each null where the agent has none, or the user unbound
 *  it. */
export type InterruptKeys = NonNullable<ReturnType<typeof interruptKeysOf>>;

/** Whether `command` has a stop-all-agents action PADE can press - its
 *  background work then needs no extra turn asking the agent to stop it. */
export function hasStopAllAction(command: string) {
  return (AGENT_INTERRUPT_SHORTCUTS[command]?.stopAll ?? null) !== null;
}

/** A shortcut ready to press: each keystroke's bytes, and how many times. */
export type KeyPress = NonNullable<InterruptKeys["interrupt"]>;

/** The user's Claude keybindings, or null - also when the file can't be read
 *  or isn't the shape Claude takes, which Claude itself then ignores too. */
async function readClaudeKeybindings() {
  try {
    return await sendMessage(Command.claudeKeybindings);
  } catch (error) {
    console.warn("pade: could not read Claude's keybindings, using its defaults", error);
    return null;
  }
}

/** `command`'s interrupt keys as the user has them now, read fresh each time
 *  so a rebinding applies without a restart. Never rejects. */
export async function interruptKeys(command: string) {
  const keybindings = command === AgentId.Claude ? await readClaudeKeybindings() : null;
  return interruptKeysOf({
    command,
    keybindings
  });
}
