import { aiNamingEnabled } from "@/lib/agent/ai-naming-scope";
import { settings } from "@/lib/settings/settings.svelte";
import { applyNamingToAll, isNaming, setNaming } from "@/lib/stores/session/sessionNaming.svelte";
import type { AgentSession } from "@/lib/types/agent";
import { untrack } from "svelte";

/** AI naming of a window's session tabs: each project's own choice, else the
 *  global default. Call during component initialisation - it registers the
 *  effect that keeps the open tabs in step with the setting. */
export function createSessionNaming({ sessions }: { sessions: () => AgentSession[] }) {
  // Whether a project names its session tabs with AI - its own choice when it
  // has one, else the global default (lib/agent/ai-naming-scope).
  function namesWithAi(project: string) {
    return aiNamingEnabled({
      prefs: settings.prefs,
      project
    });
  }

  // Name a session taken in from another window as its project says - unless
  // a ✦ toggled on by hand came along with it.
  function seedNaming(session: AgentSession) {
    if (isNaming(session.id)) {
      return;
    }

    setNaming({
      id: session.id,
      agent: session.agent.command,
      isOn: namesWithAi(session.project)
    });
  }

  // The naming settings as one comparable key, so the effect below re-runs only
  // when one of them really changes.
  const namingSettingsKey = $derived(
    JSON.stringify([settings.prefs.aiSessionNaming, settings.prefs.aiSessionNamingProjects])
  );

  // Apply that setting to every tab already open, each under its own project,
  // and again whenever it changes. `sessions` is read untracked on purpose: this
  // effect exists to follow the SETTING, and re-running it on every tab open or
  // close would undo a ✦ the user had just flipped by hand. New tabs are seeded
  // in `launch` instead.
  $effect(() => {
    applyNamingToAll({
      sessions: untrack(sessions),
      isOnFor: project => namingSettingsKey !== "" && namesWithAi(project)
    });
  });

  return {
    namesWithAi,
    seedNaming
  };
}
