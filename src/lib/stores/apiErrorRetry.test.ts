import { parseApiError } from "@/lib/stores/apiErrorRetry.svelte";
import { describe, expect, it } from "vitest";

describe("parseApiError", () => {
  describe("detects an API-side stop", () => {
    it.each([
      "⎿  API Error: 500 {\"type\":\"error\"}",
      "{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}",
      "500 Internal Server Error",
      "502 Bad Gateway",
      "503 Service Unavailable",
      "HTTP 500",
      "Error 529",
      "status: 502",
      "Connection error.",
      "connection reset by peer",
      "read ECONNRESET",
      "network error occurred"
    ])("flags %j", text => {
      expect(parseApiError({ text })).toBe(true);
    });
  });

  it("is case-insensitive", () => {
    expect(parseApiError({ text: "api error" })).toBe(true);
    expect(parseApiError({ text: "API ERROR" })).toBe(true);
    expect(parseApiError({ text: "OVERLOADED_ERROR" })).toBe(true);
    expect(parseApiError({ text: "econnreset".toUpperCase() })).toBe(true);
  });

  // The usage-limit message is auto-resume's to own — this sniffer must never
  // claim it, or the two features would fight over the same stopped session.
  describe("never fires on a usage limit", () => {
    it.each([
      "5-hour limit reached ∙ resets 3pm",
      "Claude usage limit reached. Your limit will reset at 3pm.",
      "weekly limit reached ∙ resets Oct 14",
      "Approaching usage limit · resets at 3pm"
    ])("ignores %j", text => {
      expect(parseApiError({ text })).toBe(false);
    });
  });

  describe("never fires on ordinary output", () => {
    it.each([
      "Compiled 500 modules successfully",
      "Listening on port 5000",
      "All 502 tests passed",
      "the connection was established",
      "this method is overloaded",
      "network is reachable",
      "wrote 529 bytes to disk",
      "Reading file 3 of 500",
      "Fetching 502 rows from the table",
      "done"
    ])("ignores %j", text => {
      expect(parseApiError({ text })).toBe(false);
    });
  });
});
