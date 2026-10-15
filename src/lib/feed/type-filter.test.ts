import { countExtensions, filterByExtension, isEveryTypeSelected, typeFilterLabel } from "@/lib/feed/type-filter";
import { ChangeKind } from "@/lib/types";
import type { ChangeEvent } from "@/lib/types";
import { describe, expect, it } from "vitest";

function change(path: string) {
  return {
    id: path,
    path,
    kind: ChangeKind.enum.modified,
    added: 0,
    removed: 0,
    summary: `Edited ${path}`,
    ts: 0
  } satisfies ChangeEvent;
}

const EVENTS = [
  change("src/a.ts"),
  change("src/b.css"),
  change("src/c.ts"),
  change("Makefile"),
  change("src/d.astro")
];

describe("countExtensions", () => {
  it("tallies per extension, busiest first, ties by name", () => {
    expect(countExtensions(EVENTS)).toEqual([
      {
        extension: ".ts",
        count: 2
      },
      {
        extension: ".astro",
        count: 1
      },
      {
        extension: ".css",
        count: 1
      },
      {
        extension: "Makefile",
        count: 1
      }
    ]);
  });

  it("is empty for an empty feed", () => {
    expect(countExtensions([])).toEqual([]);
  });
});

describe("isEveryTypeSelected", () => {
  const counts = countExtensions(EVENTS);

  it("is true only when every listed type is ticked", () => {
    expect(
      isEveryTypeSelected({
        counts,
        selected: new Set([".ts", ".astro", ".css", "Makefile"])
      })
    ).toBe(true);
    expect(
      isEveryTypeSelected({
        counts,
        selected: new Set([".ts"])
      })
    ).toBe(false);
  });

  it("is never true for an empty list", () => {
    expect(
      isEveryTypeSelected({
        counts: [],
        selected: new Set()
      })
    ).toBe(false);
  });
});

describe("typeFilterLabel", () => {
  const counts = countExtensions(EVENTS);

  it("reads all types with nothing or everything ticked", () => {
    expect(
      typeFilterLabel({
        counts,
        selected: new Set()
      })
    ).toBe("All types");
    expect(
      typeFilterLabel({
        counts,
        selected: new Set([".ts", ".astro", ".css", "Makefile"])
      })
    ).toBe("All types");
  });

  it("counts a partial selection", () => {
    expect(
      typeFilterLabel({
        counts,
        selected: new Set([".ts", ".css"])
      })
    ).toBe("2 of 4 types");
  });
});

describe("filterByExtension", () => {
  it("keeps every event with nothing ticked", () => {
    expect(
      filterByExtension({
        events: EVENTS,
        selected: new Set()
      })
    ).toBe(EVENTS);
  });

  it("keeps only the ticked types", () => {
    const kept = filterByExtension({
      events: EVENTS,
      selected: new Set([".ts", "Makefile"])
    });

    expect(kept.map(event => event.path)).toEqual(["src/a.ts", "src/c.ts", "Makefile"]);
  });
});
