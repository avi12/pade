import { followsSystemScheme, resolveAppearance } from "@/lib/theme-mode";
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
