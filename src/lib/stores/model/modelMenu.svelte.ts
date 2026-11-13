// The window's one model and effort menu: which session it is open for and
// which chip opened it - a tab's model tag or a pane header's model chip - so
// the menu hangs off exactly that chip. Opening it also re-reads the agent's
// catalog, so what the agent fetched since shows up.

import { refreshModelOptions } from "@/lib/stores/model/modelOptions.svelte";
import type { AgentSession } from "@/lib/types/agent";

/** The menu popover every model chip opens by this id. */
export const MODEL_MENU_ID = "model-menu";
/** The anchor the menu hangs off - set on whichever chip opened it. */
export const MODEL_MENU_ANCHOR = "--model-menu-anchor";

/** Where a model chip sits; a session's tab and its pane each carry one. */
export const ModelMenuOpener = {
  TAB: "tab",
  PANE: "pane"
} as const;
export type ModelMenuOpener = (typeof ModelMenuOpener)[keyof typeof ModelMenuOpener];

interface ModelMenuTarget {
  sessionId: string;
  opener: ModelMenuOpener;
}

let target = $state<ModelMenuTarget | null>(null);

/** Hang the menu off this chip, showing `session`'s models. */
export async function openModelMenu({ session, opener }: { session: AgentSession; opener: ModelMenuOpener }) {
  target = {
    sessionId: session.id,
    opener
  };
  await refreshModelOptions(session.agent.command);
}

/** The session the menu was last opened for, or null before any chip opened it. */
export function modelMenuSessionId() {
  return target?.sessionId ?? null;
}

/** The anchor name for this chip: the menu's while it last opened the menu,
 *  else none - two anchors of one name would pin the menu to the wrong chip. */
export function modelMenuAnchor({ sessionId, opener }: ModelMenuTarget) {
  const isOpener = target?.sessionId === sessionId && target.opener === opener;
  return isOpener ? MODEL_MENU_ANCHOR : undefined;
}
