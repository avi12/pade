// Claude's free usage-limit reset (claude.ai's "Reset for free"), shared by the
// two places that offer it - the collapsed usage meter's chip and the row in its
// menu - so both show the same offer and open the one confirmation. Spending a
// reset can't be undone, so asking for one only opens that confirmation.

import { usage as usageApi } from "@/lib/bridge";
import { errorMessage } from "@/lib/errors";
import { formatCount } from "@/lib/format";
import { showToast } from "@/lib/stores/toast.svelte";
import type { LimitReset } from "@/lib/types";

let offer = $state<LimitReset | null>(null);
let isConfirming = $state(false);
let isClaiming = $state(false);
let failure = $state<string | null>(null);

/** The reset Claude's account can spend now, or null when it holds none. */
export function limitResetOffer(): LimitReset | null {
  return offer;
}

/** How many resets are left and until when ("1 left · use by Oct 22"), or ""
 *  without an offer. */
export function limitResetSummary(): string {
  if (!offer) {
    return "";
  }

  const left = `${formatCount(offer.resetsLeft)} left`;
  if (!offer.endsAt) {
    return left;
  }

  const date = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric"
  }).format(Date.parse(offer.endsAt));
  return `${left} · use by ${date}`;
}

/** Whether the confirmation is open, and how it stands. */
export function limitResetConfirmation() {
  return {
    isOpen: isConfirming,
    isClaiming,
    failure
  };
}

/** Re-read the offer - on each of the meter's usage readings, and after a claim. */
export async function refreshLimitResetOffer(): Promise<void> {
  offer = await usageApi.limitResetOffer().catch(() => null);
  announceNewOffer();
}

// A toast the first time each grant shows up, so a reset that lands mid-session
// is noticed even with the meter out of view; never again for the same grant.
let announcedGrantId: string | null = null;

function announceNewOffer() {
  const grantId = offer?.grantId ?? null;
  const isNewOffer = grantId !== null && grantId !== announcedGrantId;
  if (!isNewOffer) {
    return;
  }

  announcedGrantId = grantId;
  showToast("A free usage-limit reset is available - click Reset on Claude's usage meter");
}

/** Open the confirmation for spending the offered reset. */
export function askToResetLimits(): void {
  failure = null;
  isConfirming = true;
}

/** Close the confirmation, unless the claim is already on its way. */
export function cancelLimitReset(): void {
  if (!isClaiming) {
    isConfirming = false;
  }
}

/** Spend the offered reset. `onreset` re-reads the account once it worked, so
 *  the meter shows the cleared limits; a refusal stays open with its reason. */
export async function confirmLimitReset(onreset: () => Promise<void>): Promise<void> {
  if (!offer) {
    return;
  }

  isClaiming = true;
  try {
    const outcome = await usageApi.claimLimitReset(offer.grantId);
    if (!outcome.reset) {
      failure = `Not reset: ${outcome.reason ?? outcome.result}`;
      return;
    }

    isConfirming = false;
    await onreset();
    await refreshLimitResetOffer();
  } catch (error) {
    failure = errorMessage({
      error,
      fallback: "Couldn't reach Claude to reset the limits."
    });
  } finally {
    isClaiming = false;
  }
}
