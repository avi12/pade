// The Windows Terminal colour-scheme catalogue, loaded once per window.
//
// One home for "which schemes exist and which one is chosen": the config
// panel's pickers render from it and `lib/prefs` resolves the chosen name
// through it, so the list the user picks from and the palette actually painted
// can never come from two different answers.

import { terminal } from "@/lib/bridge";
import type { Scheme, TerminalScheme, TerminalSchemeChoice } from "@/lib/types";

const catalogue = $state<{ schemes: TerminalScheme[] }>({ schemes: [] });

/** Every scheme the terminal can be painted with, in the backend's order. */
export function terminalSchemes(): readonly TerminalScheme[] {
  return catalogue.schemes;
}

/** Load the catalogue. Called once at startup, before the first `apply()`, so a
 *  persisted choice paints on the very first frame instead of after a flash of
 *  the app palette. A failure leaves the list empty — every scheme then
 *  resolves to "follow the app theme", which is the honest fallback. */
export async function loadTerminalSchemes(): Promise<void> {
  try {
    catalogue.schemes = await terminal.schemes();
  } catch {
    // No catalogue (an older backend, an unreadable settings file): the picker
    // shows only "Follow the app theme" and that is what the terminal wears.
    catalogue.schemes = [];
  }
}

/** The scheme chosen for `scheme`, or `null` for "follow the app theme" — which
 *  is also the answer when the name no longer names anything, so a scheme the
 *  user deleted from Windows Terminal degrades instead of painting a ghost. */
export function chosenTerminalScheme({
  choice,
  scheme
}: {
  choice: TerminalSchemeChoice | null | undefined;
  scheme: Scheme;
}): TerminalScheme | null {
  const name = choice?.[scheme];
  if (!name) {
    return null;
  }

  return catalogue.schemes.find(candidate => candidate.name === name) ?? null;
}
