import { languageIcon, ProjectKind } from "@/lib/languageIcon";
import { describe, expect, it } from "vitest";

describe("languageIcon", () => {
  it("maps each project kind to its language logo", () => {
    expect(languageIcon(ProjectKind.Web)).toBe("javascript");
    expect(languageIcon(ProjectKind.Python)).toBe("python");
    expect(languageIcon(ProjectKind.Java)).toBe("java");
    expect(languageIcon(ProjectKind.Go)).toBe("go");
    expect(languageIcon(ProjectKind.Rust)).toBe("rust");
    expect(languageIcon(ProjectKind.Android)).toBe("android");
    expect(languageIcon(ProjectKind.Cpp)).toBe("cplusplus");
    expect(languageIcon(ProjectKind.DotNet)).toBe("csharp");
    expect(languageIcon(ProjectKind.Php)).toBe("php");
    expect(languageIcon(ProjectKind.Ruby)).toBe("ruby");
  });

  it("falls back to the generic code glyph for an unknown kind", () => {
    expect(languageIcon("cobol")).toBe("code");
    expect(languageIcon("")).toBe("code");
  });
});
