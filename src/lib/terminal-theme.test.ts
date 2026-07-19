import { xtermSafeColor, xtermTheme } from "@/lib/terminal-theme";
import { describe, expect, it } from "vitest";

describe("xtermSafeColor", () => {
  it("converts primary hues exactly", () => {
    expect(xtermSafeColor("hsl(0 100% 50%)")).toBe("#ff0000");
    expect(xtermSafeColor("hsl(120deg 100% 50%)")).toBe("#00ff00");
    expect(xtermSafeColor("hsl(240deg 100% 50%)")).toBe("#0000ff");
  });

  it("converts the achromatic ends", () => {
    expect(xtermSafeColor("hsl(0 0% 100%)")).toBe("#ffffff");
    expect(xtermSafeColor("hsl(214deg 0% 0%)")).toBe("#000000");
  });

  it("converts a mid-sextant hue", () => {
    // h=210 → sextant 3 (blue-cyan): rgb(0, 128, 255).
    expect(xtermSafeColor("hsl(210deg 100% 50%)")).toBe("#0080ff");
  });

  it("keeps a percent alpha as an 8-digit hex — the selection token that xterm's own parser rejected", () => {
    // 22% → 0.22 × 255 ≈ 56 → 0x38.
    expect(xtermSafeColor("hsl(210deg 90% 45% / 22%)")).toMatch(/^#[0-9a-f]{6}38$/);
  });

  it("accepts a decimal alpha too", () => {
    expect(xtermSafeColor("hsl(0 0% 0% / 0.5)")).toBe("#00000080");
    expect(xtermSafeColor("hsl(0 0% 0% / .5)")).toBe("#00000080");
  });

  it("drops a fully-opaque alpha instead of appending ff", () => {
    expect(xtermSafeColor("hsl(0 100% 50% / 100%)")).toBe("#ff0000");
    expect(xtermSafeColor("hsl(0 100% 50% / 1)")).toBe("#ff0000");
  });

  it("normalizes an out-of-range hue", () => {
    expect(xtermSafeColor("hsl(480deg 100% 50%)")).toBe(xtermSafeColor("hsl(120deg 100% 50%)"));
    expect(xtermSafeColor("hsl(-120deg 100% 50%)")).toBe(xtermSafeColor("hsl(240deg 100% 50%)"));
  });

  it("passes every non-hsl format through untouched for xterm's own parser", () => {
    expect(xtermSafeColor("#aabbcc")).toBe("#aabbcc");
    expect(xtermSafeColor("rgb(1, 2, 3)")).toBe("rgb(1, 2, 3)");
    expect(xtermSafeColor("color-mix(in srgb, red, blue)")).toBe("color-mix(in srgb, red, blue)");
    expect(xtermSafeColor("")).toBe("");
  });
});

describe("xtermTheme", () => {
  const tokens: Record<string, string> = {
    "--code-background": "hsl(214deg 40% 6%)",
    "--code-foreground": "hsl(210deg 20% 85%)",
    "--primary": "hsl(210deg 90% 68%)",
    "--terminal-selection": "hsl(210deg 90% 66% / 28%)"
  };

  const theme = xtermTheme({ readToken: name => tokens[name] ?? "hsl(0 0% 50%)" });

  it("maps the code surface and cursor tokens onto xterm's slots", () => {
    expect(theme.background).toBe(xtermSafeColor(tokens["--code-background"]!));
    expect(theme.foreground).toBe(xtermSafeColor(tokens["--code-foreground"]!));
    expect(theme.cursor).toBe(xtermSafeColor(tokens["--primary"]!));
  });

  it("hands xterm an alpha selection it can actually parse (8-digit hex)", () => {
    expect(theme.selectionBackground).toMatch(/^#[0-9a-f]{8}$/);
  });

  it("fills all 16 ANSI slots with parse-safe hex", () => {
    const slots = [
      theme.black, theme.red, theme.green, theme.yellow,
      theme.blue, theme.magenta, theme.cyan, theme.white,
      theme.brightBlack, theme.brightRed, theme.brightGreen, theme.brightYellow,
      theme.brightBlue, theme.brightMagenta, theme.brightCyan, theme.brightWhite
    ];
    for (const slot of slots) {
      expect(slot).toMatch(/^#[0-9a-f]{6}$/);
    }
  });
});
