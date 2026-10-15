import { createScreenSampler } from "@/lib/terminal/screen-sampler";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const INTERVAL_MS = 1000;

describe("createScreenSampler", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(10_000);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("samples at once, then folds a burst into one trailing sample", () => {
    const sample = vi.fn();
    const sampler = createScreenSampler({
      intervalMs: INTERVAL_MS,
      sample
    });

    sampler.request();
    expect(sample).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(200);
    sampler.request();
    sampler.request();
    expect(sample).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(800);
    expect(sample).toHaveBeenCalledTimes(2);
  });

  it("drops the trailing sample once disposed", () => {
    const sample = vi.fn();
    const sampler = createScreenSampler({
      intervalMs: INTERVAL_MS,
      sample
    });

    sampler.request();
    sampler.request();
    sampler.dispose();
    vi.advanceTimersByTime(INTERVAL_MS);
    expect(sample).toHaveBeenCalledTimes(1);
  });
});
