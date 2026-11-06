import { Updater } from "@/lib/types";
import { updateStatusLine } from "@/lib/update/status-line";
import { describe, expect, it } from "vitest";

const CURRENT = "0.11.1";

describe("updateStatusLine", () => {
  it("waits for the backend's first answer", () => {
    expect(updateStatusLine(null)).toBe("Reading the version…");
  });

  it("says an MSIX install is updated by Windows", () => {
    expect(
      updateStatusLine({
        current: CURRENT,
        ready: null,
        updater: Updater.enum.windows
      })
    ).toBe("PADE 0.11.1 · installed from its Windows package, so updates come through Windows");
  });

  it("says a development build never updates itself", () => {
    expect(
      updateStatusLine({
        current: CURRENT,
        ready: null,
        updater: Updater.enum.none
      })
    ).toBe("PADE 0.11.1 · a development build, which never updates itself");
  });

  it("names a ready release, else says the copy is up to date", () => {
    expect(
      updateStatusLine({
        current: CURRENT,
        ready: "0.11.2",
        updater: Updater.enum.pade
      })
    ).toBe("PADE 0.11.1 · 0.11.2 is ready and installs when PADE closes");
    expect(
      updateStatusLine({
        current: CURRENT,
        ready: null,
        updater: Updater.enum.pade
      })
    ).toBe("PADE 0.11.1 · up to date");
  });
});
