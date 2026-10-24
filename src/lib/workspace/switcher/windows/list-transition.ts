// Run a list mutation (pin/unpin/remove/delete/clear/reorder - each an async
// update that reshapes the rows) inside a view transition scoped to *just* the
// rows container, so a row morphs - one moved between sections glides to its new
// place - instead of snapping. Scoped to that element, never `document`, so it
// never snapshots the live-repainting terminal (which would ghost). Reduced
// motion, or an engine without the scoped API, runs the mutation directly.

import { prefersReducedMotion } from "@/lib/ui/motion";
import { tick } from "svelte";

type ScopedTransitionList = Element & {
  startViewTransition?: (callback: () => Promise<void>) => { updateCallbackDone: Promise<void> };
};

export async function runListTransition({ elList, mutate }: {
  elList: ScopedTransitionList | null;
  mutate: () => Promise<void>;
}) {
  if (prefersReducedMotion() || !elList?.startViewTransition) {
    await mutate();
    return;
  }

  // Resolve once the DOM has updated (so a caller like the delete flow can close
  // its dialog then) - the transition itself keeps animating after that.
  const transition = elList.startViewTransition(async () => {
    await mutate();
    await tick();
  });
  await transition.updateCallbackDone;
}
