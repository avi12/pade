import { settledValue } from "@/lib/util/async/settled";
import { describe, expect, it } from "vitest";

describe("settledValue", () => {
  it("passes a resolved value through", async () => {
    await expect(
      settledValue({
        promise: Promise.resolve(["main"]),
        fallback: []
      })
    ).resolves.toEqual(["main"]);
  });

  it("answers the fallback when the promise rejects", async () => {
    await expect(
      settledValue({
        promise: Promise.reject(new Error("not a git repository")),
        fallback: []
      })
    ).resolves.toEqual([]);
  });
});
