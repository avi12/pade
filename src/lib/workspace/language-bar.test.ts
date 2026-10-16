import { isLanguageMix, languageShareText } from "@/lib/workspace/language-bar";
import { describe, expect, it } from "vitest";

const SHARES = [
  {
    kind: "dotnet",
    label: "C# / .NET",
    perMille: 920
  },
  {
    kind: "rust",
    label: "Rust",
    perMille: 80
  }
];

describe("isLanguageMix", () => {
  it("draws no bar for a project that is essentially one language", () => {
    expect(isLanguageMix(SHARES)).toBe(false);
    expect(isLanguageMix(SHARES.slice(0, 1))).toBe(false);
  });

  it("draws a bar once two languages are major", () => {
    expect(
      isLanguageMix([
        {
          kind: "web",
          label: "Web / JavaScript",
          perMille: 600
        },
        {
          kind: "python",
          label: "Python",
          perMille: 350
        },
        {
          kind: "dotnet",
          label: "C# / .NET",
          perMille: 50
        }
      ])
    ).toBe(true);
  });
});

describe("languageShareText", () => {
  it("names a segment's language and share", () => {
    expect(languageShareText(SHARES[0])).toBe("C# / .NET 92%");
  });
});
