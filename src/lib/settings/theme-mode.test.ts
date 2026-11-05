import {
  adoptedMonospaceFace,
  followsSystemScheme,
  PALETTE_MONOSPACE_FACE,
  resolveAppearance
} from "@/lib/settings/theme-mode";
import { Settings, ThemeMode, ThemePalette } from "@/lib/types";
import { describe, expect, it } from "vitest";

describe("resolveAppearance", () => {
  it("takes a fixed mode at its word, whatever the OS says", () => {
    expect(
      resolveAppearance({
        mode: ThemeMode.enum.light,
        palette: ThemePalette.enum.default,
        systemScheme: "dark"
      })
    ).toEqual({
      scheme: "light",
      palette: ThemePalette.enum.default
    });
    expect(
      resolveAppearance({
        mode: ThemeMode.enum.dark,
        palette: ThemePalette.enum.default,
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
        palette: ThemePalette.enum.default,
        systemScheme: "dark"
      }).scheme
    ).toBe("dark");
  });

  it("lays every skin over every mode, so Cyberpunk has a day and a night face", () => {
    for (const systemScheme of ["light", "dark"] as const) {
      expect(
        resolveAppearance({
          mode: ThemeMode.enum.system,
          palette: ThemePalette.enum.cyberpunk,
          systemScheme
        })
      ).toEqual({
        scheme: systemScheme,
        palette: ThemePalette.enum.cyberpunk
      });
    }

    expect(
      resolveAppearance({
        mode: ThemeMode.enum.light,
        palette: ThemePalette.enum.cyberpunk,
        systemScheme: "dark"
      })
    ).toEqual({
      scheme: "light",
      palette: ThemePalette.enum.cyberpunk
    });
  });
});

describe("followsSystemScheme", () => {
  it("is only Auto - Light and Dark are schemes of their own", () => {
    expect(followsSystemScheme(ThemeMode.enum.system)).toBe(true);
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
          palette: ThemePalette.enum.cyberpunk,
          current
        })
      ).toBe(cyberpunkFace);
    }
  });

  it("releases a face that came from a skin when leaving it", () => {
    expect(
      adoptedMonospaceFace({
        palette: ThemePalette.enum.default,
        current: cyberpunkFace
      })
    ).toBeNull();
  });

  it("leaves a font the user chose themselves alone", () => {
    for (const current of ["JetBrains Mono", "", null, undefined]) {
      expect(
        adoptedMonospaceFace({
          palette: ThemePalette.enum.default,
          current
        })
      ).toBeUndefined();
    }
  });
});

describe("stored theme prefs", () => {
  const SAVED_SETTINGS = {
    roots: [],
    defaultAgent: null,
    projectAgents: {}
  };

  function loadedPrefs(prefs: Record<string, unknown>) {
    return Settings.parse({
      ...SAVED_SETTINGS,
      prefs
    }).prefs;
  }

  it("reads the old Cyberpunk theme mode as Auto wearing the Cyberpunk skin", () => {
    expect(
      loadedPrefs({
        themeMode: ThemePalette.enum.cyberpunk,
        monoFont: "Share Tech Mono"
      })
    ).toEqual({
      themeMode: ThemeMode.enum.system,
      themePalette: ThemePalette.enum.cyberpunk,
      monoFont: "Share Tech Mono"
    });
  });

  it("keeps both axes as saved once they are split", () => {
    expect(
      loadedPrefs({
        themeMode: ThemeMode.enum.light,
        themePalette: ThemePalette.enum.cyberpunk
      })
    ).toEqual({
      themeMode: ThemeMode.enum.light,
      themePalette: ThemePalette.enum.cyberpunk
    });
    expect(loadedPrefs({})).toEqual({});
  });
});
