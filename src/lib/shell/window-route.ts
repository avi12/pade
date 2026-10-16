// The window's address, `?w=<mode>[&path=<project>]`: what a spawned window
// boots into, and - kept truthful by the shell as the user moves on - what a
// reload re-routes off. One module reads and writes it so the two can't drift.

import { WindowMode } from "@/lib/types";
import { FolderPath, parseInput } from "@/lib/validate";

const MODE_PARAMETER = "w";
const PATH_PARAMETER = "path";

/** Where a window's address sends it on boot. */
export const WindowRoute = {
  /** A fresh throwaway workspace. */
  Temp: "temp",
  /** The project picker. */
  Picker: "picker",
  /** A named project folder. */
  Project: "project",
  /** No usable address - the plain main window's launch-context boot. */
  LaunchContext: "launch-context"
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
    return { kind: WindowRoute.Temp } as const;
  }

  if (mode === WindowMode.enum.empty) {
    return { kind: WindowRoute.Picker } as const;
  }

  const path = mode === WindowMode.enum.open
    ? parseInput({
      schema: FolderPath,
      raw: query.get(PATH_PARAMETER)
    })
    : null;
  if (path === null) {
    return { kind: WindowRoute.LaunchContext } as const;
  }

  return {
    kind: WindowRoute.Project,
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
