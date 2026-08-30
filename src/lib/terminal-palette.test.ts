import { schemeOfBackground, TERMINAL_PALETTE_PROPERTIES, terminalPaletteProperties } from "@/lib/terminal-palette";
import { ANSI_COLOR_TOKENS } from "@/lib/terminal-theme";
import type { TerminalScheme } from "@/lib/types";
import { describe, expect, it } from "vitest";

/** Solarized Dark's palette, as Windows Terminal ships it. */
const solarizedDark: TerminalScheme = {
  name: "Solarized Dark",
  background: "#002B36",
  foreground: "#839496",
  cursor: "#FFFFFF",
  selection: null,
  ansi: [
    "#002B36",
    "#DC322F",
    "#859900",
    "#B58900",
    "#268BD2",
    "#D33682",
    "#2AA198",
    "#EEE8D5",
    "#073642",
    "#CB4B16",
    "#586E75",
    "#657B83",
    "#839496",
    "#6C71C4",
    "#93A1A1",
    "#FDF6E3"
  ]
};

describe("schemeOfBackground", () => {
  it("reads a scheme's own background, not the app's", () => {
    expect(
      schemeOfBackground({
        color: solarizedDark.background,
        fallback: "light"
      })
    ).toBe("dark");
    expect(
      schemeOfBackground({
        color: "#FDF6E3",
        fallback: "dark"
      })
    ).toBe("light");
  });

  it("handles the extremes and the short form", () => {
    expect(
      schemeOfBackground({
        color: "#000",
        fallback: "light"
      })
    ).toBe("dark");
    expect(
      schemeOfBackground({
        color: "#fff",
        fallback: "dark"
      })
    ).toBe("light");
  });

  it("falls back rather than guessing at a colour it cannot read", () => {
    for (const color of ["", "rgb(0 0 0)", "not-a-colour", "#12345"]) {
      expect(
        schemeOfBackground({
          color,
          fallback: "dark"
        })
      ).toBe("dark");
      expect(
        schemeOfBackground({
          color,
          fallback: "light"
        })
      ).toBe("light");
    }
  });
});

describe("terminalPaletteProperties", () => {
  it("maps the 16 ANSI colours onto the tokens in ANSI order", () => {
    const properties = terminalPaletteProperties(solarizedDark);
    for (let index = 0; index < ANSI_COLOR_TOKENS.length; index++) {
      expect(properties.get(ANSI_COLOR_TOKENS[index])).toBe(solarizedDark.ansi[index]);
    }
  });

  it("paints the terminal surface and the caret the scheme names", () => {
    const properties = terminalPaletteProperties(solarizedDark);
    expect(properties.get("--code-background")).toBe("#002B36");
    expect(properties.get("--code-foreground")).toBe("#839496");
    expect(properties.get("--terminal-cursor")).toBe("#FFFFFF");
  });

  it("derives a translucent selection when the scheme names none", () => {
    // Windows Terminal composites its own there, so a missing one must not
    // leave xterm to invent the white wash that hides light-scheme text.
    const properties = terminalPaletteProperties(solarizedDark);
    expect(properties.get("--terminal-selection")).toMatch(/^#839496[0-9a-f]{2}$/);
  });

  it("falls the caret back to the foreground for a scheme that names none", () => {
    const properties = terminalPaletteProperties({
      ...solarizedDark,
      cursor: null
    });
    expect(properties.get("--terminal-cursor")).toBe(solarizedDark.foreground);
  });

  it("hands the syntax roles the slots the scheme already spends on them", () => {
    // The code viewers share `--code-background` with the terminal, so a scheme
    // that moves the paper has to move the ink with it.
    const properties = terminalPaletteProperties(solarizedDark);
    expect(properties.get("--syntax-keyword")).toBe(solarizedDark.ansi[5]);
    expect(properties.get("--syntax-string")).toBe(solarizedDark.ansi[2]);
    expect(properties.get("--syntax-number")).toBe(solarizedDark.ansi[3]);
    expect(properties.get("--syntax-function")).toBe(solarizedDark.ansi[4]);
    expect(properties.get("--syntax-property")).toBe(solarizedDark.ansi[6]);
    // Comments take bright black — the muted grey every palette reserves for
    // them — and never a bright colour, which a light scheme spends on greys.
    expect(properties.get("--syntax-comment")).toBe(solarizedDark.ansi[8]);
  });

  it("writes exactly the properties the uninstall clears", () => {
    // The two lists have to agree or picking "follow the app theme" again would
    // leave a stray token behind, painting half of one palette over the other.
    expect([...terminalPaletteProperties(solarizedDark).keys()].sort()).toEqual(
      [...TERMINAL_PALETTE_PROPERTIES].sort()
    );
  });
});
