import {
  adoptedMonospaceFace,
  followsSystemScheme,
  PALETTE_MONOSPACE_FACE,
  paletteOfMode,
  resolveAppearance
} from "@/lib/theme-mode";
import { ThemeMode, ThemePalette } from "@/lib/types";
import { describe, expect, it } from "vitest";

describe("resolveAppearance", () => {
  it("takes a fixed mode at its word, whatever the OS says", () => {
    expect(
      resolveAppearance({
        mode: ThemeMode.enum.light,
        systemScheme: "dark"
      })
    ).toEqual({
      scheme: "light",
      palette: ThemePalette.enum.default
    });
    expect(
      resolveAppearance({
        mode: ThemeMode.enum.dark,
        systemScheme: "light"
      })
    ).toEqual({
      scheme: "dark",
      palette: ThemePalette.enum.default
    });
  });

  it("follows the OS on auto", () => {
    expect(
      resolveAppearance({
        mode: ThemeMode.enum.system,
        systemScheme: "dark"
      }).scheme
    ).toBe("dark");
  });

  it("gives cyberpunk the OS scheme, so the skin has a day and a night face", () => {
    for (const systemScheme of ["light", "dark"] as const) {
      expect(
        resolveAppearance({
          mode: ThemeMode.enum.cyberpunk,
          systemScheme
        })
      ).toEqual({
        scheme: systemScheme,
        palette: ThemePalette.enum.cyberpunk
      });
    }
  });
});

describe("followsSystemScheme", () => {
  it("covers the two modes that carry no scheme of their own", () => {
    expect(followsSystemScheme(ThemeMode.enum.system)).toBe(true);
    expect(followsSystemScheme(ThemeMode.enum.cyberpunk)).toBe(true);
    expect(followsSystemScheme(ThemeMode.enum.light)).toBe(false);
    expect(followsSystemScheme(ThemeMode.enum.dark)).toBe(false);
  });
});

describe("PALETTE_MONOSPACE_FACE", () => {
  it("leaves the default palette on the theme's own stack and gives Cyberpunk a face", () => {
    expect(PALETTE_MONOSPACE_FACE[ThemePalette.enum.default]).toBeNull();
    expect(PALETTE_MONOSPACE_FACE[ThemePalette.enum.cyberpunk]).toBe("Share Tech Mono");
  });
});

describe("adoptedMonospaceFace", () => {
  const cyberpunkFace = PALETTE_MONOSPACE_FACE[ThemePalette.enum.cyberpunk]!;

  it("takes the skin's own face when the skin ships one", () => {
    for (const current of [null, undefined, "", "JetBrains Mono"]) {
      expect(
        adoptedMonospaceFace({
          mode: ThemeMode.enum.cyberpunk,
          current
        })
      ).toBe(cyberpunkFace);
    }
  });

  it("releases a face that came from a skin when leaving it", () => {
    expect(
      adoptedMonospaceFace({
        mode: ThemeMode.enum.light,
        current: cyberpunkFace
      })
    ).toBeNull();
  });

  it("leaves a font the user chose themselves alone", () => {
    for (const current of ["JetBrains Mono", "", null, undefined]) {
      expect(
        adoptedMonospaceFace({
          mode: ThemeMode.enum.dark,
          current
        })
      ).toBeUndefined();
    }
  });
});

describe("paletteOfMode", () => {
  it("is the skin axis alone — only Cyberpunk names one", () => {
    expect(paletteOfMode(ThemeMode.enum.cyberpunk)).toBe(ThemePalette.enum.cyberpunk);
    for (const mode of [ThemeMode.enum.light, ThemeMode.enum.dark, ThemeMode.enum.system]) {
      expect(paletteOfMode(mode)).toBe(ThemePalette.enum.default);
    }
  });
});
