// Questions about the other PADE windows that are open right now.

import type { WindowInfo } from "@/lib/types";
import { normalizePath } from "@/lib/util/paths";

/** Whether another window has this project open. Its agents hold the folder,
 *  and this window can't stop them, so nothing here may move it. */
export function isOpenInAnotherWindow({ windowRows, path }: {
  windowRows: readonly WindowInfo[];
  path: string;
}) {
  return windowRows.some(windowRow => !windowRow.isCurrent
    && normalizePath(windowRow.path) === normalizePath(path));
}
