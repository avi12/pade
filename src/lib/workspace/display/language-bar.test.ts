import { describe, expect, it } from "vitest";

import {
  isEnteringAfterCount,
  isLanguageMix,
  languageShareText,
  segmentColors
} from "@/lib/workspace/display/language-bar";

function share({ language, perMille }: { language: string; perMille: number }) {
  return {
    language,
    kind: "web",
    perMille
  };
}

describe("isLanguageMix", () => {
  it("draws no bar for a project that is essentially one language", () => {
    expect(
      isLanguageMix([
        share({
          language: "C#",
          perMille: 920
        }),
        share({
          language: "Rust",
          perMille: 80
        })
      ])
    ).toBe(false);
    expect(
      isLanguageMix([
        share({
          language: "TypeScript",
          perMille: 1000
        })
      ])
    ).toBe(false);
  });

  it("draws a bar once two languages are major", () => {
    expect(
      isLanguageMix([
        share({
          language: "TypeScript",
          perMille: 600
        }),
        share({
          language: "Svelte",
          perMille: 350
        }),
        share({
          language: "CSS",
          perMille: 50
        })
      ])
    ).toBe(true);
  });
});

describe("languageShareText", () => {
  it("names a segment's language and share", () => {
    expect(
      languageShareText(
        share({
          language: "Svelte",
          perMille: 345
        })
      )
    ).toBe("Svelte 35%");
  });
});

describe("segmentColors", () => {
  it("keeps each language's own colour when they already stand apart", () => {
    expect(
      segmentColors([
        share({
          language: "TypeScript",
          perMille: 600
        }),
        share({
          language: "Svelte",
          perMille: 400
        })
      ])
    ).toEqual(["#3178c6", "#ff3e00"]);
  });

  it("swaps to the alternate when two colours of one bar would look alike", () => {
    expect(
      segmentColors([
        share({
          language: "TypeScript",
          perMille: 600
        }),
        share({
          language: "Python",
          perMille: 400
        })
      ])
    ).toEqual(["#3178c6", "#ffd43b"]);
  });

  it("gives an unknown language a neutral colour", () => {
    expect(
      segmentColors([
        share({
          language: "Zig",
          perMille: 1000
        })
      ])
    ).toEqual(["#8a9bb0"]);
  });
});

describe("isEnteringAfterCount", () => {
  const mix = [
    share({
      language: "C#",
      perMille: 600
    }),
    share({
      language: "Rust",
      perMille: 400
    })
  ];

  it("grows in when a row's first count lands on screen", () => {
    expect(
      isEnteringAfterCount({
        previous: undefined,
        current: mix,
        isEntering: false,
        isOnScreen: true
      })
    ).toBe(true);
  });

  it("paints at rest when the count lands ahead of the scroll", () => {
    expect(
      isEnteringAfterCount({
        previous: undefined,
        current: mix,
        isEntering: false,
        isOnScreen: false
      })
    ).toBe(false);
  });

  it("grows in when a recount on screen finds a new language", () => {
    expect(
      isEnteringAfterCount({
        previous: [
          share({
            language: "C#",
            perMille: 1000
          })
        ],
        current: mix,
        isEntering: false,
        isOnScreen: true
      })
    ).toBe(true);
  });

  it("stays at rest when only the shares move", () => {
    expect(
      isEnteringAfterCount({
        previous: [
          share({
            language: "C#",
            perMille: 500
          }),
          share({
            language: "Rust",
            perMille: 500
          })
        ],
        current: mix,
        isEntering: false,
        isOnScreen: true
      })
    ).toBe(false);
  });

  it("ends the entrance when the count has no bar to draw", () => {
    expect(
      isEnteringAfterCount({
        previous: undefined,
        current: [
          share({
            language: "C#",
            perMille: 1000
          })
        ],
        isEntering: true,
        isOnScreen: true
      })
    ).toBe(false);
  });
});
