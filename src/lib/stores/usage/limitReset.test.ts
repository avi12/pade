import { showToast, toastText } from "@/lib/stores/ui/toast.svelte";
import {
  askToResetLimits,
  cancelLimitReset,
  confirmLimitReset,
  limitResetConfirmation,
  limitResetOffer,
  limitResetSummary,
  refreshLimitResetOffer
} from "@/lib/stores/usage/limitReset.svelte";
import type { LimitReset } from "@/lib/types";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const bridgeMocks = vi.hoisted(() => ({
  limitResetOffer: vi.fn<() => Promise<LimitReset | null>>(),
  claimLimitReset: vi.fn<(grantId: string) => Promise<{
    reset: boolean;
    result: string;
    reason?: string | null;
  }>>()
}));

vi.mock("@/lib/bridge", () => ({ usage: bridgeMocks }));

const launchReset: LimitReset = {
  grantId: "opus55-launch-promax-20260921",
  label: "Claude Opus 5.5 launch: one usage-limit reset for Pro and Max",
  resetsLeft: 1,
  endsAt: "2026-10-22T16:00:00+00:00",
  clears: ["five_hour", "seven_day"]
};

beforeEach(async () => {
  vi.clearAllMocks();
  bridgeMocks.limitResetOffer.mockResolvedValue(launchReset);
  await refreshLimitResetOffer();
  cancelLimitReset();
});

describe("the free limit reset", () => {
  it("offers the account's reset with how many are left", () => {
    expect(limitResetOffer()?.grantId).toBe(launchReset.grantId);
    expect(limitResetSummary()).toContain("1 left");
  });

  it("only asks, never spends, when the reset is requested", () => {
    askToResetLimits();

    expect(limitResetConfirmation().isOpen).toBe(true);
    expect(bridgeMocks.claimLimitReset).not.toHaveBeenCalled();
  });

  it("spends the reset once confirmed, then re-reads the account", async () => {
    bridgeMocks.claimLimitReset.mockResolvedValue({
      reset: true,
      result: "reset"
    });
    bridgeMocks.limitResetOffer.mockResolvedValue(null);
    const onreset = vi.fn(async () => {});
    askToResetLimits();

    await confirmLimitReset(onreset);

    expect(bridgeMocks.claimLimitReset).toHaveBeenCalledWith(launchReset.grantId);
    expect(onreset).toHaveBeenCalled();
    expect(limitResetConfirmation().isOpen).toBe(false);
    expect(limitResetOffer()).toBeNull();
  });

  it("stays open with the reason when the server refuses", async () => {
    bridgeMocks.claimLimitReset.mockResolvedValue({
      reset: false,
      result: "ineligible",
      reason: "not_limited"
    });
    askToResetLimits();

    await confirmLimitReset(vi.fn(async () => {}));

    expect(limitResetConfirmation()).toEqual({
      isOpen: true,
      isClaiming: false,
      failure: "Not reset: not_limited"
    });
  });

  it("stays open with an error when Claude can't be reached", async () => {
    bridgeMocks.claimLimitReset.mockRejectedValue(new Error("offline"));
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
    bridgeMocks.limitResetOffer.mockResolvedValue(freshReset);

    await refreshLimitResetOffer();
    expect(toastText()).toContain("free usage-limit reset is available");

    showToast("");
    await refreshLimitResetOffer();
    expect(toastText()).toBe("");
  });
});
