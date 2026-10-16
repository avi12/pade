// Questions about the other PADE windows that are open right now.

import type { WindowInfo } from "@/lib/types";
import { normalizePath } from "@/lib/util/paths";

/** The other window that has this project open, if any. */
export function otherWindowWith({ windowRows, path }: {
  windowRows: readonly WindowInfo[];
  path: string;
}) {
  return windowRows.find(windowRow => !windowRow.isCurrent
    && normalizePath(windowRow.path) === normalizePath(path));
}

/** Whether another window has this project open. Its agents hold the folder,
 *  and this window can't stop them, so nothing here may move it. */
export function isOpenInAnotherWindow(target: Parameters<typeof otherWindowWith>[0]) {
  return otherWindowWith(target) !== undefined;
}
