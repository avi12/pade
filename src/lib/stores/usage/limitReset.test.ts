import { Command } from "@/lib/ipc/messaging";
import { showToast, toastText } from "@/lib/stores/ui/toast.svelte";
import {
  askToResetLimits,
  cancelLimitReset,
  chooseLimitReset,
  confirmLimitReset,
  limitResetConfirmation,
  limitResetOffers,
  refreshLimitResetOffers
} from "@/lib/stores/usage/limitReset.svelte";
import type { LimitReset } from "@/lib/types";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn<(command: string, data?: unknown) => Promise<unknown>>(),
  limitResetOffers: vi.fn<() => Promise<LimitReset[]>>(),
  claimLimitReset: vi.fn<() => Promise<{
    reset: boolean;
    result: string;
    reason?: string | null;
  }>>()
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage
}));

function routeMessage(command: string) {
  if (command === Command.limitResetOffers) {
    return mocks.limitResetOffers();
  }

  if (command === Command.limitResetClaim) {
    return mocks.claimLimitReset();
  }

  throw new Error(`unexpected command ${command}`);
}

const launchReset: LimitReset = {
  grantId: "opus55-launch-promax-20260921",
  label: "Claude Opus 5.5 launch: one usage-limit reset for Pro and Max",
  resetsLeft: 1,
  endsAt: "2026-10-22T16:00:00+00:00",
  clears: ["five_hour", "seven_day"]
};

beforeEach(async () => {
  vi.clearAllMocks();
  mocks.sendMessage.mockImplementation(routeMessage);
  mocks.limitResetOffers.mockResolvedValue([launchReset]);
  await refreshLimitResetOffers();
  cancelLimitReset();
});

describe("the free limit reset", () => {
  it("offers the account's reset with how many are left", () => {
    expect(limitResetOffers().map(offer => offer.grantId)).toEqual([launchReset.grantId]);
  });

  it("only asks, never spends, when the reset is requested", () => {
    askToResetLimits();

    expect(limitResetConfirmation().isOpen).toBe(true);
    expect(mocks.claimLimitReset).not.toHaveBeenCalled();
  });

  it("spends the reset once confirmed, then re-reads the account", async () => {
    mocks.claimLimitReset.mockResolvedValue({
      reset: true,
      result: "reset"
    });
    mocks.limitResetOffers.mockResolvedValue([]);
    const onreset = vi.fn(async () => {});
    askToResetLimits();

    await confirmLimitReset(onreset);

    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.limitResetClaim, { grantId: launchReset.grantId });
    expect(onreset).toHaveBeenCalled();
    expect(limitResetConfirmation().isOpen).toBe(false);
    expect(limitResetOffers()).toEqual([]);
  });

  it("stays open with the reason when the server refuses", async () => {
    mocks.claimLimitReset.mockResolvedValue({
      reset: false,
      result: "ineligible",
      reason: "not_limited"
    });
    askToResetLimits();

    await confirmLimitReset(vi.fn(async () => {}));

    expect(limitResetConfirmation()).toMatchObject({
      isOpen: true,
      isClaiming: false,
      failure: "Not reset: not_limited"
    });
  });

  it("stays open with an error when Claude can't be reached", async () => {
    mocks.claimLimitReset.mockRejectedValue(new Error("offline"));
    askToResetLimits();

    await confirmLimitReset(vi.fn(async () => {}));

    expect(limitResetConfirmation().failure).toBeTruthy();
    expect(limitResetConfirmation().isOpen).toBe(true);
  });
});

describe("announcing a free limit reset", () => {
  it("toasts the first time a reset shows up, and not again for the same one", async () => {
    const freshReset = {
      ...launchReset,
      grantId: "another-launch-grant"
    };
    mocks.limitResetOffers.mockResolvedValue([freshReset]);

    await refreshLimitResetOffers();
    expect(toastText()).toContain("full reset is available");

    showToast("");
    await refreshLimitResetOffers();
    expect(toastText()).toBe("");
  });
});

describe("choosing between kinds of reset", () => {
  const fableReset: LimitReset = {
    grantId: "launch-2610-fable",
    label: "New model launch: one Fable reset",
    resetsLeft: 3,
    endsAt: "2027-03-09T07:59:00+00:00",
    clears: ["seven_day_overage_included"]
  };

  beforeEach(async () => {
    mocks.limitResetOffers.mockResolvedValue([fableReset, launchReset]);
    await refreshLimitResetOffers();
  });

  it("opens on the first kind, or on the one a menu row asked for", () => {
    askToResetLimits();
    expect(limitResetConfirmation().chosen?.grantId).toBe(fableReset.grantId);

    cancelLimitReset();
    askToResetLimits(launchReset.grantId);
    expect(limitResetConfirmation().chosen?.grantId).toBe(launchReset.grantId);
  });

  it("spends the kind picked in the confirmation", async () => {
    mocks.claimLimitReset.mockResolvedValue({
      reset: true,
      result: "reset"
    });
    askToResetLimits();
    chooseLimitReset(launchReset.grantId);

    await confirmLimitReset(vi.fn(async () => {}));

    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.limitResetClaim, { grantId: launchReset.grantId });
  });
});
