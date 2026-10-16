import { isLanguageMix, languageShareText, segmentColors } from "@/lib/workspace/language-bar";
import { describe, expect, it } from "vitest";

function share(language: string, perMille: number) {
  return {
    language,
    kind: "web",
    perMille
  };
}

describe("isLanguageMix", () => {
  it("draws no bar for a project that is essentially one language", () => {
    expect(isLanguageMix([share("C#", 920), share("Rust", 80)])).toBe(false);
    expect(isLanguageMix([share("TypeScript", 1000)])).toBe(false);
  });

  it("draws a bar once two languages are major", () => {
    expect(isLanguageMix([share("TypeScript", 600), share("Svelte", 350), share("CSS", 50)])).toBe(true);
  });
});

describe("languageShareText", () => {
  it("names a segment's language and share", () => {
    expect(languageShareText(share("Svelte", 345))).toBe("Svelte 35%");
  });
});

describe("segmentColors", () => {
  it("keeps each language's own colour when they already stand apart", () => {
    expect(segmentColors([share("TypeScript", 600), share("Svelte", 400)])).toEqual(["#3178c6", "#ff3e00"]);
  });

  it("swaps to the alternate when two colours of one bar would look alike", () => {
    expect(segmentColors([share("TypeScript", 600), share("Python", 400)])).toEqual(["#3178c6", "#ffd43b"]);
  });

  it("gives an unknown language a neutral colour", () => {
    expect(segmentColors([share("Zig", 1000)])).toEqual(["#8a9bb0"]);
  });
});
