import { tick } from "svelte";

// Which top-level screen is showing. A closed set defined once, compared
// against by member so no bare literal leaks into the flow logic.
export const Phase = {
  Loading: "loading",
  Project: "project",
  Onboarding: "onboarding",
  Ready: "ready"
} as const;
type Phase = (typeof Phase)[keyof typeof Phase];

/** On the root while the picker morphs into the workspace (ProjectSidebar). */
const ENTERING_WORKSPACE_CLASS = "entering-workspace";

/** The window's top-level screen, and the way between them. */
export function createScreenPhase() {
  let phase = $state<Phase>(Phase.Loading);
  // Carried through the agent picker so a new-project prompt survives onboarding.
  let pendingPrompt = $state<string | undefined>();

  // The one way into the workspace screen. Leaving the picker morphs through a
  // view transition with the project rail held still (ProjectSidebar), instead of
  // the screen snapping; from anywhere else it simply shows.
  async function showWorkspace() {
    const isLeavingPicker = phase === Phase.Project;
    if (!isLeavingPicker) {
      phase = Phase.Ready;
      return;
    }

    document.documentElement.classList.add(ENTERING_WORKSPACE_CLASS);
    const transition = document.startViewTransition(async () => {
      phase = Phase.Ready;
      await tick();
    });
    await transition.finished;
    document.documentElement.classList.remove(ENTERING_WORKSPACE_CLASS);
  }

  function switchToPicker() {
    document.startViewTransition(async () => {
      phase = Phase.Project;
      await tick();
    });
  }

  return {
    get phase() {
      return phase;
    },
    get pendingPrompt() {
      return pendingPrompt;
    },
    setPhase(next: Phase) {
      phase = next;
    },
    clearPendingPrompt() {
      pendingPrompt = undefined;
    },
    showWorkspace,
    switchToPicker
  };
}

export type ScreenPhase = ReturnType<typeof createScreenPhase>;
