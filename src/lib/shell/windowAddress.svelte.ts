// What a window says about itself, kept true as the user moves on: the session
// snapshot a reload re-attaches from, the `w=` address it re-routes off, and the
// OS window title. All three follow the shell's state; none of them is read back
// until the next boot (or by the OS).

import { setWindowTitle } from "@/lib/ipc/platform";
import { saveSessionSnapshot } from "@/lib/session/session-restore";
import { settings } from "@/lib/settings/settings.svelte";
import { windowQueryFor } from "@/lib/shell/window-route";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { displayName } from "@/lib/util/paths";

export function syncWindowAddress({ isBooted, project, isUnnamedTemp, windowSessions }: {
  /** False while the window is still booting - nothing is true yet. */
  isBooted: () => boolean;
  /** The window's open project; empty while none is. */
  project: () => string;
  /** A temp workspace that never earned a name - titled as the bare product. */
  isUnnamedTemp: () => boolean;
  windowSessions: WindowSessions;
}) {
  // Persist this window's pane mapping so a reload can re-attach it. Never while
  // still booting - the effect's first run lands before the restore has read the
  // snapshot, and would wipe it. Once live, an empty project or session list
  // clears the snapshot instead: nothing to re-attach.
  $effect(() => {
    if (!isBooted()) {
      return;
    }

    saveSessionSnapshot({
      project: project(),
      sessions: windowSessions.sessions,
      paneIds: windowSessions.paneIds,
      activeId: windowSessions.activeId
    });
  });

  // The address is state too: `w=` is what a reload re-routes off, and it went
  // stale the moment the user moved on from what the window was spawned for -
  // a `?w=empty` picker window living inside a project rebooted to the picker
  // (the incident), a `?w=temp` one minted a second throwaway. Keep the query
  // telling the truth - the project on screen, else the picker - so even a
  // snapshot-less reload (all sessions had exited) lands on the right screen.
  $effect(() => {
    if (!isBooted()) {
      return;
    }

    const query = windowQueryFor(project());
    if (location.search !== query) {
      history.replaceState(null, "", query);
    }
  });

  // Name the OS window (title bar + taskbar) `PADE - <workspace>`, where the
  // workspace is the open project's own folder name - the part after the slash,
  // honoring a friendly label - reusing the same `displayName` leaf the switcher
  // rows show. The empty picker and a throwaway temp that never earned a name stay
  // the bare product name. Driven off the project, so it re-titles on every
  // open, switch, and close.
  const hasNamedProject = $derived(project() !== "" && !isUnnamedTemp());
  $effect(() => {
    const workspace = displayName({
      path: project(),
      labels: settings.labels
    });
    setWindowTitle(hasNamedProject ? `PADE - ${workspace}` : "PADE");
  });
}
