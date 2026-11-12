import { Updater } from "@/lib/types/update";
import { INSTALL_MSIX_NOTICE, updateStatusLine } from "@/lib/update/status-line";
import { describe, expect, it } from "vitest";

const CURRENT = "0.11.1";
const NEWER = {
  version: "0.11.2",
  releaseUrl: "https://github.com/avi12/pade/releases/tag/v0.11.2"
};

describe("updateStatusLine", () => {
  it("waits for the backend's first answer", () => {
    expect(updateStatusLine(null)).toBe("Reading the version…");
  });

  it("says an MSIX install is updated by Windows, even when a newer release exists", () => {
    expect(
      updateStatusLine({
        current: CURRENT,
        newer: NEWER,
        updater: Updater.enum.windows
      })
    ).toBe("PADE 0.11.1 · installed from its Windows package, so updates come through Windows");
  });

  it("says a development build never updates itself, and never shows the notice", () => {
    expect(
      updateStatusLine({
        current: CURRENT,
        newer: NEWER,
        updater: Updater.enum.none
      })
    ).toBe("PADE 0.11.1 · a development build, which never updates itself");
  });

  it("tells an unpackaged copy to install the package once a newer release exists", () => {
    expect(
      updateStatusLine({
        current: CURRENT,
        newer: NEWER,
        updater: Updater.enum.unpackaged
      })
    ).toBe(`PADE 0.11.1 · ${INSTALL_MSIX_NOTICE}`);
  });

  it("says an unpackaged copy with no newer release is up to date", () => {
    expect(
      updateStatusLine({
        current: CURRENT,
        newer: null,
        updater: Updater.enum.unpackaged
      })
    ).toBe("PADE 0.11.1 · up to date");
  });
});
