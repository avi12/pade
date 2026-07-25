import { collectVars, resolveColor } from "@/lib/colors";
import { describe, expect, it, vi } from "vitest";

// `resolveColor` gates on `CSS.supports("color", …)`, which Node lacks. A
// minimal stand-in for the engine's parser keeps these tests focused on what
// colors.ts owns: var() tracing, depth capping and fallbacks.
vi.stubGlobal("CSS", {
  supports(_property: string, value: string) {
    return value.startsWith("#") || value.startsWith("rgb");
  }
});

describe("collectVars", () => {
  it("collects custom-property declarations from stylesheet text", () => {
    const variables = collectVars(":root { --brand: #123456; --gap: 4px; color: red; }");

    expect(variables.get("--brand")).toBe("#123456");
    expect(variables.get("--gap")).toBe("4px");
    expect(variables.size).toBe(2);
  });

  it("trims whitespace around values", () => {
    const variables = collectVars("--spacing:   12px  ;");

    expect(variables.get("--spacing")).toBe("12px");
  });

  it("ignores declarations without a terminating semicolon", () => {
    expect(collectVars("--brand: #fff").size).toBe(0);
  });
});

describe("resolveColor", () => {
  it("returns a literal color as-is", () => {
    expect(resolveColor("#3366ff")).toBe("#3366ff");
  });

  it("rejects text that is not a color", () => {
    expect(resolveColor("banana")).toBeNull();
  });

  it("traces a var() through the provided token map", () => {
    const variables = collectVars("--brand: #123456;");

    expect(resolveColor("var(--brand)", variables)).toBe("#123456");
  });

  it("follows nested var() references", () => {
    const variables = collectVars("--alias: var(--base); --base: rgb(1, 2, 3);");

    expect(resolveColor("var(--alias)", variables)).toBe("rgb(1, 2, 3)");
  });

  it("tolerates whitespace inside the var() reference", () => {
    const variables = collectVars("--brand: #fff;");

    expect(resolveColor("var( --brand )", variables)).toBe("#fff");
  });

  it("gives up on a circular var() chain instead of recursing forever", () => {
    const variables = collectVars("--one: var(--two); --two: var(--one);");

    expect(resolveColor("var(--one)", variables)).toBeNull();
  });

  it("returns null for an unknown var() with no document to fall back to", () => {
    expect(resolveColor("var(--missing)", new Map())).toBeNull();
  });
});
