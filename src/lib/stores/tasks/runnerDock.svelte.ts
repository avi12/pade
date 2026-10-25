// How tall the runner dock stands for a project - the one source both the dock
// and the terminal panes read. Each project's runners are its own, so the dock
// comes and goes as projects take the front; a terminal kept for a project in
// the background is sized against THAT project's dock, so bringing it to the
// front never resizes it (no reflow, no jump).

import { projectRunnerRows } from "@/lib/stores/tasks/runners.svelte";

// Clamped so the dock can never swallow the whole window nor collapse below a
// usable minimum.
const MINIMUM_DOCK_HEIGHT = 140;
const INITIAL_DOCK_HEIGHT = 360;
const MAXIMUM_DOCK_VIEWPORT_SHARE = 0.75;
// Header + one attached card's bar + its note, no output pane.
const COMPACT_DOCK_HEIGHT = 132;

function clampDock(height: number) {
  return Math.min(Math.max(height, MINIMUM_DOCK_HEIGHT), innerHeight * MAXIMUM_DOCK_VIEWPORT_SHARE);
}

// The height the user dragged the streaming dock to, shared by every project.
let resizableHeight = $state(clampDock(INITIAL_DOCK_HEIGHT));

export const runnerDock = {
  get resizableHeight() {
    return resizableHeight;
  },
  /** Drag the streaming dock to `height`, clamped to the viewport. */
  resizeTo(height: number) {
    resizableHeight = clampDock(height);
  },
  /** Re-clamp after the window resized under a stored height. */
  reclamp() {
    resizableHeight = clampDock(resizableHeight);
  }
};

/** Whether every card of the project is an ATTACHED runner (a task the AGENT
 *  runs, tracked but not owned): no streaming output, just a one-line note. */
export function isAttachedOnlyDock(project: string) {
  const rows = projectRunnerRows(project);
  return rows.length > 0 && rows.every(row => row.attached);
}

/** The dock's height while `project` is in front: none without runners,
 *  compact for attached ones only - so it never squeezes the terminal above it
 *  for a one-line note - and the resizable height once any task streams. */
export function dockBlockSize(project: string) {
  if (projectRunnerRows(project).length === 0) {
    return 0;
  }

  if (isAttachedOnlyDock(project)) {
    return COMPACT_DOCK_HEIGHT;
  }

  return resizableHeight;
}
