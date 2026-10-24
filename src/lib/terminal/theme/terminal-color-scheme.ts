// The terminal color-scheme notification Claude Code subscribes to with DECSET
// 2031. xterm paints its new palette immediately, but changing `options.theme`
// does not emit this application-facing status report by itself.

import { AgentId } from "@/lib/agent/agent-icon";
import { CONTROL_SEQUENCE_INTRODUCER } from "@/lib/terminal/input/terminal-sequences";
import { Scheme } from "@/lib/types";

const COLOR_SCHEME_STATUS = "?997";
const COLOR_SCHEME_NOTIFICATIONS = "?2031h";
const DEVICE_STATUS_REPORT = "n";
const DARK_SCHEME_CODE = 1;
const LIGHT_SCHEME_CODE = 2;
const COLOR_SCHEME_ENABLE = `${CONTROL_SEQUENCE_INTRODUCER}${COLOR_SCHEME_NOTIFICATIONS}`;

// Agents whose TUI subscribes to DEC 2031 color-scheme reports at startup
// (Claude's `auto` theme, opencode's renderer). The re-attach path treats
// them as subscribed even when the handshake was trimmed out of the replayed
// history. (The report only carries the scheme; opencode's own light/dark
// *detection* is spawn-themed instead - see agents.rs - because its mode
// probe is answered by ConPTY, not by ADE.)
const SCHEME_SUBSCRIBED_AGENTS = new Set<string>([AgentId.Claude, AgentId.Opencode]);

/** Report the current terminal color preference to a TUI that opted into DECSET
 * 2031 notifications. */
export function colorSchemeReport(scheme: Scheme) {
  const code = scheme === Scheme.enum.dark ? DARK_SCHEME_CODE : LIGHT_SCHEME_CODE;
  return `${CONTROL_SEQUENCE_INTRODUCER}${COLOR_SCHEME_STATUS};${code}${DEVICE_STATUS_REPORT}`;
}

/** Whether a TUI chunk enables the DEC 2031 color-scheme notification channel. */
export function enablesColorSchemeNotifications(data: string) {
  return data.includes(COLOR_SCHEME_ENABLE);
}

/** One pane's DEC 2031 channel: watches the agent's output for the handshake
 *  and, once subscribed, relays the scheme report on every repaint that asks.
 *  Any agent that enables the channel gets the truthful report - the handshake
 *  itself is the opt-in, not the agent's id. */
export function createColorSchemeRelay({ agentId, report }: {
  /** The pane's agent, read when deciding whether it listens without a handshake. */
  agentId: () => string;
  /** Write the current scheme's report to the agent. */
  report: () => Promise<void>;
}) {
  // A PTY read can cut a control sequence in half, so retain one complete
  // sequence's worth of tail while looking for the handshake.
  let notificationTail = "";
  let isEnabled = false;

  return {
    /** Look for the agent's handshake in a chunk of its output (or a replay). */
    observe(data: string) {
      if (isEnabled) {
        return;
      }

      const combined = notificationTail + data;
      notificationTail = combined.slice(-(COLOR_SCHEME_ENABLE.length - 1));

      if (!enablesColorSchemeNotifications(combined)) {
        return;
      }

      isEnabled = true;
      report();
    },
    /** A session attached mid-flight (or entering its fullscreen frame) whose
     *  agent is known to listen gets the report even with no handshake seen. */
    async reportToSubscribedAgent() {
      const listensWithoutHandshake = !isEnabled && SCHEME_SUBSCRIBED_AGENTS.has(agentId());
      if (!listensWithoutHandshake) {
        return;
      }

      isEnabled = true;
      await report();
    },
    /** The palette changed - tell an agent that subscribed. */
    relayChange() {
      if (isEnabled) {
        report();
      }
    }
  };
}
