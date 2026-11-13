// The window's address, `?w=<mode>[&path=<project>]`: what a spawned window
// boots into, and - kept truthful by the shell as the user moves on - what a
// reload re-routes off. One module reads and writes it so the two can't drift.

import { WindowMode } from "@/lib/types/window";
import { FolderPath, parseInput } from "@/lib/validate";

const MODE_PARAMETER = "w";
const PATH_PARAMETER = "path";

/** Where a window's address sends it on boot. */
export const WindowRoute = {
  /** A fresh throwaway workspace. */
  TEMP: "temp",
  /** The project picker. */
  PICKER: "picker",
  /** A named project folder. */
  PROJECT: "project",
  /** A window spawned to receive sessions another window handed it, in the
   *  named project folder (lib/shell/handoff/window-handoff). */
  HANDOFF: "handoff",
  /** No usable address - the plain main window's launch-context boot. */
  LAUNCH_CONTEXT: "launch-context"
} as const;

/** Whether the address is the agents dashboard's - a window that boots its own
 *  small app instead of a project window's. */
export function isDashboardAddress(search: string) {
  return new URLSearchParams(search).get(MODE_PARAMETER) === WindowMode.enum.dashboard;
}

/** Read a window address (`location.search`). The path is a trust boundary, so
 *  an `open` address without a valid folder falls back to the launch context. */
export function windowRouteFor(search: string) {
  const query = new URLSearchParams(search);
  const mode = query.get(MODE_PARAMETER);
  if (mode === WindowMode.enum.temp) {
    return { kind: WindowRoute.TEMP } as const;
  }

  if (mode === WindowMode.enum.empty) {
    return { kind: WindowRoute.PICKER } as const;
  }

  const isProjectMode = mode === WindowMode.enum.open || mode === WindowMode.enum.handoff;
  const path = isProjectMode
    ? parseInput({
        schema: FolderPath,
        raw: query.get(PATH_PARAMETER)
      })
    : null;
  if (path === null) {
    return { kind: WindowRoute.LAUNCH_CONTEXT } as const;
  }

  if (mode === WindowMode.enum.handoff) {
    return {
      kind: WindowRoute.HANDOFF,
      path
    } as const;
  }

  return {
    kind: WindowRoute.PROJECT,
    path
  } as const;
}

/** The address that tells the truth about a window: the project on screen,
 *  else the picker. */
export function windowQueryFor(project: string) {
  if (project === "") {
    return `?${MODE_PARAMETER}=${WindowMode.enum.empty}`;
  }

  return `?${MODE_PARAMETER}=${WindowMode.enum.open}&${PATH_PARAMETER}=${encodeURIComponent(project)}`;
}
