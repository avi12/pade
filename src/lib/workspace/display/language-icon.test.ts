import { describe, expect, it } from "vitest";

import { languageIcon, languageIconOf, ProjectKind } from "@/lib/workspace/display/language-icon";

describe("languageIcon", () => {
  it("maps each project kind to its language logo", () => {
    expect(languageIcon(ProjectKind.WEB)).toBe("javascript");
    expect(languageIcon(ProjectKind.PYTHON)).toBe("python");
    expect(languageIcon(ProjectKind.JAVA)).toBe("java");
    expect(languageIcon(ProjectKind.GO)).toBe("go");
    expect(languageIcon(ProjectKind.RUST)).toBe("rust");
    expect(languageIcon(ProjectKind.ANDROID)).toBe("android");
    expect(languageIcon(ProjectKind.CPP)).toBe("cplusplus");
    expect(languageIcon(ProjectKind.DOT_NET)).toBe("csharp");
    expect(languageIcon(ProjectKind.PHP)).toBe("php");
    expect(languageIcon(ProjectKind.RUBY)).toBe("ruby");
  });

  it("shows the TypeScript badge for the backend's TS narrowing", () => {
    expect(languageIcon("typescript")).toBe("typescript");
  });

  it("falls back to the generic code glyph for an unknown kind", () => {
    expect(languageIcon("cobol")).toBe("code");
    expect(languageIcon("")).toBe("code");
  });
});

describe("languageIconOf", () => {
  it("maps a counted language to its own logo, and an unknown one to the code glyph", () => {
    expect(languageIconOf("Kotlin")).toBe("kotlin");
    expect(languageIconOf("SCSS")).toBe("css");
    expect(languageIconOf("Zig")).toBe("code");
  });
});
