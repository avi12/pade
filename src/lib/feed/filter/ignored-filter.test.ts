import { describe, expect, it } from "vitest";

import { filterIgnored, isHidingIgnored } from "@/lib/feed/filter/ignored-filter";
import { ChangeKind } from "@/lib/types/feed";
import type { ChangeEvent } from "@/lib/types/feed";

function change({ path, isIgnored }: { path: string; isIgnored: boolean }) {
  return {
    id: path,
    root: "C:/repo",
    path,
    kind: ChangeKind.enum.modified,
    added: 1,
    removed: 0,
    summary: "+1",
    ts: 0,
    isIgnored
  } satisfies ChangeEvent;
}

describe("isHidingIgnored", () => {
  it("hides by default where a .gitignore exists", () => {
    expect(
      isHidingIgnored({
        prefs: {},
        hasGitignore: true
      })
    ).toBe(true);
  });

  it("shows everything once turned off", () => {
    expect(
      isHidingIgnored({
        prefs: {
          hideIgnoredChanges: false
        },
        hasGitignore: true
      })
    ).toBe(false);
  });

  it("hides nothing without a .gitignore", () => {
    expect(
      isHidingIgnored({
        prefs: {
          hideIgnoredChanges: true
        },
        hasGitignore: false
      })
    ).toBe(false);
  });
});

describe("filterIgnored", () => {
  const events = [
    change({
      path: "src/app.ts",
      isIgnored: false
    }),
    change({
      path: "out/app.js",
      isIgnored: true
    })
  ];

  it("leaves the ignored out while hiding", () => {
    expect(
      filterIgnored({
        events,
        isHiding: true
      }).map(event => event.path)
    ).toEqual(["src/app.ts"]);
  });

  it("keeps every event otherwise", () => {
    expect(
      filterIgnored({
        events,
        isHiding: false
      })
    ).toBe(events);
  });
});
