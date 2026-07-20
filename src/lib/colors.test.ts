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
    const vars = collectVars(":root { --brand: #123456; --gap: 4px; color: red; }");

    expect(vars.get("--brand")).toBe("#123456");
    expect(vars.get("--gap")).toBe("4px");
    expect(vars.size).toBe(2);
  });

  it("trims whitespace around values", () => {
    const vars = collectVars("--spacing:   12px  ;");

    expect(vars.get("--spacing")).toBe("12px");
  });

  it("ignores declarations without a terminating semicolon", () => {
    expect(collectVars("--brand: #fff").size).toBe(0);
  });
});

describe("resolveColor", () => {
  it("returns a literal color as-is", () => {
    expect(resolveColor({ token: "#3366ff" })).toBe("#3366ff");
  });

  it("rejects text that is not a color", () => {
    expect(resolveColor({ token: "banana" })).toBeNull();
  });

  it("traces a var() through the provided token map", () => {
    const vars = collectVars("--brand: #123456;");

    expect(resolveColor({ token: "var(--brand)", vars })).toBe("#123456");
  });

  it("follows nested var() references", () => {
    const vars = collectVars("--alias: var(--base); --base: rgb(1, 2, 3);");

    expect(resolveColor({ token: "var(--alias)", vars })).toBe("rgb(1, 2, 3)");
  });

  it("tolerates whitespace inside the var() reference", () => {
    const vars = collectVars("--brand: #fff;");

    expect(resolveColor({ token: "var( --brand )", vars })).toBe("#fff");
  });

  it("gives up on a circular var() chain instead of recursing forever", () => {
    const vars = collectVars("--one: var(--two); --two: var(--one);");

    expect(resolveColor({ token: "var(--one)", vars })).toBeNull();
  });

  it("returns null for an unknown var() with no document to fall back to", () => {
    expect(resolveColor({ token: "var(--missing)", vars: new Map() })).toBeNull();
  });
});
