import { resolvedStartMode } from "@/lib/settings/start-mode";
import { StartMode } from "@/lib/types";
import { describe, expect, it } from "vitest";

describe("resolvedStartMode", () => {
  it("lands a direct launch on the homescreen when nothing was chosen", () => {
    expect(resolvedStartMode({ startMode: undefined })).toBe(StartMode.enum.picker);
    expect(resolvedStartMode({ startMode: null })).toBe(StartMode.enum.picker);
  });

  it("honors the temp workspace choice", () => {
    expect(resolvedStartMode({ startMode: StartMode.enum.temp })).toBe(StartMode.enum.temp);
  });

  it("honors the picker choice", () => {
    expect(resolvedStartMode({ startMode: StartMode.enum.picker })).toBe(StartMode.enum.picker);
  });
});
