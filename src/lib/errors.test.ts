import { errorMessage } from "@/lib/errors";
import { describe, expect, it } from "vitest";

describe("errorMessage", () => {
  it("passes a non-blank string through untouched", () => {
    const message = errorMessage({
      error: "the backend said no",
      fallback: "Something went wrong."
    });

    expect(message).toBe("the backend said no");
  });

  it("falls back for a whitespace-only string", () => {
    const message = errorMessage({
      error: "   ",
      fallback: "Something went wrong."
    });

    expect(message).toBe("Something went wrong.");
  });

  it("uses an Error's own message", () => {
    const message = errorMessage({
      error: new Error("watcher died"),
      fallback: "Something went wrong."
    });

    expect(message).toBe("watcher died");
  });

  it("falls back for an Error with an empty message", () => {
    const message = errorMessage({
      error: new Error(""),
      fallback: "Something went wrong."
    });

    expect(message).toBe("Something went wrong.");
  });

  it("falls back for a thrown object that is not an Error", () => {
    const message = errorMessage({
      error: {
        code: 500
      },
      fallback: "Something went wrong."
    });

    expect(message).toBe("Something went wrong.");
  });

  it("falls back for a thrown number", () => {
    const message = errorMessage({
      error: 404,
      fallback: "Something went wrong."
    });

    expect(message).toBe("Something went wrong.");
  });
});
