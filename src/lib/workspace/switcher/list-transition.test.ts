import { runListTransition } from "@/lib/workspace/switcher/list-transition";
import {
  afterEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const REDUCED_MOTION = { matches: true };
const FULL_MOTION = { matches: false };

function scopedList() {
  const startViewTransition = vi.fn((callback: () => Promise<void>) => ({ updateCallbackDone: callback() }));
  return {
    // The node test environment has no DOM; only the list's scoped
    // startViewTransition is ever read, so a bare object stands in for it.
    elList: { startViewTransition } as unknown as Element,
    startViewTransition
  };
}

describe("runListTransition", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("runs the mutation directly without a list element", async () => {
    const mutate = vi.fn(async () => {});
    await runListTransition({
      elList: null,
      mutate
    });
    expect(mutate).toHaveBeenCalledOnce();
  });

  it("wraps the mutation in the list's own view transition", async () => {
    vi.stubGlobal("matchMedia", () => FULL_MOTION);
    const { elList, startViewTransition } = scopedList();
    const mutate = vi.fn(async () => {});
    await runListTransition({
      elList,
      mutate
    });
    expect(startViewTransition).toHaveBeenCalledOnce();
    expect(mutate).toHaveBeenCalledOnce();
  });

  it("skips the transition under reduced motion", async () => {
    vi.stubGlobal("matchMedia", () => REDUCED_MOTION);
    const { elList, startViewTransition } = scopedList();
    const mutate = vi.fn(async () => {});
    await runListTransition({
      elList,
      mutate
    });
    expect(startViewTransition).not.toHaveBeenCalled();
    expect(mutate).toHaveBeenCalledOnce();
  });
});
