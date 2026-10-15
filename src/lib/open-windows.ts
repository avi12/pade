// Questions about the other PADE windows that are open right now.

import { normalizePath } from "@/lib/paths";
import type { WindowInfo } from "@/lib/types";

/** Whether another window has this project open. Its agents hold the folder,
 *  and this window can't stop them, so nothing here may move it. */
export function isOpenInAnotherWindow({ windowRows, path }: {
  windowRows: readonly WindowInfo[];
  path: string;
}) {
  return windowRows.some(windowRow => !windowRow.isCurrent
    && normalizePath(windowRow.path) === normalizePath(path));
}
