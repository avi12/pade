import { linkSegments } from "@/lib/runners/runner-links";
import { describe, expect, it } from "vitest";

describe("linkSegments", () => {
  it("leaves a line with no URL untouched", () => {
    const segments = [{ text: "npm notice run vite" }];
    expect(linkSegments(segments)).toEqual(segments);
  });

  it("cuts a URL out of a segment, leaving sentence punctuation outside it", () => {
    expect(linkSegments([{ text: "  Local:   http://localhost:5173/." }])).toEqual([
      { text: "  Local:   " },
      {
        text: "http://localhost:5173/",
        url: "http://localhost:5173/"
      },
      { text: "." }
    ]);
  });

  it("links every piece of a URL that crosses a style change", () => {
    const url = "http://localhost:5173/";
    expect(
      linkSegments([
        { text: "at http://localhost:" },
        {
          text: "5173/ now",
          bold: true
        }
      ])
    ).toEqual([
      { text: "at " },
      {
        text: "http://localhost:",
        url
      },
      {
        text: "5173/",
        bold: true,
        url
      },
      {
        text: " now",
        bold: true
      }
    ]);
  });
});
