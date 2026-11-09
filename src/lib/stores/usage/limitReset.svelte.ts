// Claude's free usage-limit resets (claude.ai's "Reset for free"), one offer
// per kind, shared by the two places that offer them - the collapsed usage
// meter's chip and the rows in its menu - so both show the same offers and
// open the one confirmation. Spending a reset can't be undone, so asking for
// one only opens that confirmation, where the user picks which kind to spend.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import type { LimitReset } from "@/lib/types";
import { limitResetKind } from "@/lib/usage/limit-reset/limit-reset-kind";
import { settledValue } from "@/lib/util/async/settled";
import { errorMessage } from "@/lib/util/errors/errors";
import { SvelteSet } from "svelte/reactivity";

let offers = $state<LimitReset[]>([]);
let isConfirming = $state(false);
let isClaiming = $state(false);
let failure = $state<string | null>(null);
let chosenGrantId = $state<string | null>(null);

/** The resets Claude's account can spend now, one per kind (the server's
 *  next kind first) - empty when it holds none. */
export function limitResetOffers() {
  return offers;
}

/** Whether the confirmation is open, which offer it would spend, and how it
 *  stands. */
export function limitResetConfirmation() {
  return {
    isOpen: isConfirming,
    isClaiming,
    failure,
    chosen: offers.find(offer => offer.grantId === chosenGrantId) ?? offers[0] ?? null
  };
}

/** Re-read the offers - on each of the meter's usage readings, and after a claim. */
export async function refreshLimitResetOffers() {
  offers = await settledValue({
    promise: sendMessage(Command.limitResetOffers),
    fallback: []
  });
  announceNewOffers();
}

// A toast the first time each grant shows up, so a reset that lands mid-session
// is noticed even with the meter out of view; never again for the same grant.
const announcedGrantIds = new SvelteSet<string>();

function announceNewOffers() {
  const fresh = offers.filter(offer => !announcedGrantIds.has(offer.grantId));
  for (const offer of fresh) {
    announcedGrantIds.add(offer.grantId);
  }

  const [first] = fresh;
  if (!first) {
    return;
  }

  showToast(`A ${limitResetKind(first.clears).name} is available - use it from Claude's usage meter`);
}

/** Open the confirmation, on `grantId`'s kind when given (a menu row's own
 *  button) or else on the first offer. */
export function askToResetLimits(grantId?: string) {
  failure = null;
  chosenGrantId = grantId ?? offers[0]?.grantId ?? null;
  isConfirming = true;
}

/** Pick which kind the open confirmation spends. */
export function chooseLimitReset(grantId: string) {
  if (!isClaiming) {
    chosenGrantId = grantId;
    failure = null;
  }
}

/** Close the confirmation, unless the claim is already on its way. */
export function cancelLimitReset() {
  if (!isClaiming) {
    isConfirming = false;
  }
}

/** Spend the chosen reset. `onreset` re-reads the account once it worked, so
 *  the meter shows the cleared limits; a refusal stays open with its reason. */
export async function confirmLimitReset(onreset: () => Promise<void>) {
  const { chosen } = limitResetConfirmation();
  if (!chosen) {
    return;
  }

  isClaiming = true;
  try {
    const outcome = await sendMessage(Command.limitResetClaim, { grantId: chosen.grantId });
    if (!outcome.reset) {
      failure = `Not reset: ${outcome.reason ?? outcome.result}`;
      return;
    }

    isConfirming = false;
    await onreset();
    await refreshLimitResetOffers();
  } catch (error) {
    failure = errorMessage({
      error,
      fallback: "Couldn't reach Claude to use the reset"
    });
  } finally {
    isClaiming = false;
  }
}
