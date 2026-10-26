// Following a terminal-scheme flip into the agents already running (SoC: the
// policy and its bookkeeping live here; the shell owns the sessions and lends
// the actions that change them).
//
// Claude Code is re-themed in place: `agent_theme_publish` rewrites the theme
// definition its sessions were launched on, and it repaints itself - no
// restart, no lost turn. Every OTHER agent's scheme is fixed at spawn and
// cannot be moved after it: Codex's syntax theme and console-buffer ground, and
// opencode's forced tui-config theme (its DECSET 2031 `?997` report only asks an
// agent to re-probe OSC 11, a query that never reaches the terminal - see
// docs/terminal-rendering.md). So for those two a flip is followed the only way
// it can be: respawn the idle ones, each resuming its own conversation
// (`codex resume <uuid>` / `opencode --session <id>` via `agent_resume_args`).
//
// Claude must NEVER join them, and this is the second time that has been settled.
// It has no resume args of its own - it is respawned onto the `--session-id` the
// dead process was just pinned to, and in the live app that lands on a blank
// pane stuck at "Starting…", the session wedged rather than re-themed. A
// portable-pty harness that starts/kills/respawns on the same id at 0ms / 250ms
// / 1500ms says all three survive; the app says otherwise, and the app is the
// one the user has to use. It no longer matters: Claude follows the flip through
// its watched theme definition instead, without being restarted at all.

import { AgentId } from "@/lib/agent/agent-icon";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { registerSessionState, SessionStateKey } from "@/lib/session/lifecycle/session-scope";
import { appearance } from "@/lib/settings/prefs.svelte";
import { isSessionIdle, whenSessionIdle } from "@/lib/stores/session/sessions.svelte";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import type { AgentSession, Scheme } from "@/lib/types";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

const SPAWN_THEMED_AGENTS = new Set<string>([AgentId.Codex, AgentId.Opencode]);

/** What the shell does on the re-themer's behalf. */
interface AgentRethemeHost {
  sessions: () => readonly AgentSession[];
  /** Hand a session the args that resume its conversation, before a restart
   *  remounts it. A no-op for an empty list. */
  applyResumeArgs: (options: {
    id: string;
    args: string[];
  }) => void;
  /** Kill the sessions and respawn them in place under fresh ids. */
  restartSessions: (targets: readonly AgentSession[]) => Promise<void>;
}

export function createAgentRetheme(host: AgentRethemeHost) {
  // The scheme each live session's agent was actually spawned under - the one
  // answer to "is this agent painted in the wrong scheme", so a session that
  // sat out one flip and its way back is never restarted for nothing. Recorded
  // once per session as it appears (a restart re-keys, so the replacement
  // records the scheme it really launched with) and dropped with the session.
  const spawnSchemeBySession = new SvelteMap<string, Scheme>();

  // A session mid-turn must not be respawned - that would sever the work in
  // flight - so its flip is held until it reaches an idle prompt, which is the
  // first moment the agent can be relaunched into the new scheme. Without this,
  // a busy agent kept painting its old scheme (dark diffs on a light terminal)
  // until the user happened to restart it by hand.
  const awaitingRetheme = new SvelteSet<string>();

  registerSessionState(SessionStateKey.enum.retheme, {
    forget(id) {
      spawnSchemeBySession.delete(id);
    }
  });

  let lastFollowedScheme = appearance.terminalScheme;

  async function rethemeWhenIdle(id: string) {
    if (awaitingRetheme.has(id)) {
      return;
    }

    awaitingRetheme.add(id);
    await whenSessionIdle({ id });
    awaitingRetheme.delete(id);

    const isStillOpen = host.sessions().some(session => session.id === id);
    if (isStillOpen) {
      await restartMismatchedAgents();
    }
  }

  function isPaintedInStaleScheme(session: AgentSession) {
    return SPAWN_THEMED_AGENTS.has(session.agent.id)
      && spawnSchemeBySession.get(session.id) !== appearance.terminalScheme;
  }

  async function restartMismatchedAgents() {
    const mismatched = host.sessions().filter(isPaintedInStaleScheme);
    for (const session of mismatched.filter(each => !isSessionIdle(each.id))) {
      rethemeWhenIdle(session.id);
    }

    const targets = mismatched.filter(session => isSessionIdle(session.id));
    if (targets.length > 0) {
      await restartOnOwnConversation(targets);
    }
  }

  async function restartOnOwnConversation(targets: AgentSession[]) {
    // Resume args land BEFORE the re-key remounts each terminal, so the respawn
    // reopens the same conversation on the new scheme instead of a blank one.
    const resumable = await Promise.all(
      targets.map(async target => ({
        id: target.id,
        args: await sendMessage(Command.agentResumeArgs, {
          command: target.agent.command,
          cwd: target.cwd ?? target.project
        })
      }))
    );
    for (const entry of resumable) {
      host.applyResumeArgs(entry);
    }

    const restartedLabel = targets.length === 1 ? targets[0].agent.label : `${targets.length} sessions`;
    showToast(`Theme changed - restarting ${restartedLabel} to match`);
    await host.restartSessions(targets);
  }

  // Live-themed agents repaint where they stand; the spawn-themed ones can only
  // be respawned. Publishing first means an agent that IS restarted also
  // launches into the new scheme.
  async function followScheme(scheme: Scheme) {
    await sendMessage(Command.agentThemePublish, { scheme });
    await restartMismatchedAgents();
  }

  return {
    /** Record the scheme each newly seen session spawned under. Run in an
     *  `$effect`, so it re-fires as sessions appear. */
    recordSpawnSchemes() {
      const scheme = appearance.terminalScheme;
      for (const session of host.sessions()) {
        if (!spawnSchemeBySession.has(session.id)) {
          spawnSchemeBySession.set(session.id, scheme);
        }
      }
    },
    /** Carry a scheme flip into the running agents. Run in an `$effect`, so it
     *  re-fires on every flip. */
    followSchemeChange() {
      const scheme = appearance.terminalScheme;
      if (scheme === lastFollowedScheme) {
        return;
      }

      lastFollowedScheme = scheme;
      followScheme(scheme);
    }
  };
}
