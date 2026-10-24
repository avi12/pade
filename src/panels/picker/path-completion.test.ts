import { EMPTY_PATH_PROBE } from "@/lib/types";
import { nextSegmentCompletion } from "@/panels/picker/path-completion";
import { describe, expect, it } from "vitest";

const TYPED = String.raw`C:\repositories\pa`;

function probeOf({ path = TYPED, completion }: {
  path?: string;
  completion: string;
}) {
  return {
    path,
    result: {
      ...EMPTY_PATH_PROBE,
      completion
    }
  };
}

describe("nextSegmentCompletion", () => {
  it("fills the segment every matching folder shares", () => {
    const completion = String.raw`C:\repositories\pad`;
    expect(
      nextSegmentCompletion({
        typed: TYPED,
        probe: probeOf({ completion })
      })
    ).toBe(completion);
  });

  it("drills into the only matching folder", () => {
    const completion = `${String.raw`C:\repositories\pade`}\\`;
    expect(
      nextSegmentCompletion({
        typed: TYPED,
        probe: probeOf({ completion })
      })
    ).toBe(completion);
  });

  it("fills nothing when the folders fork right where the text ends", () => {
    expect(
      nextSegmentCompletion({
        typed: TYPED,
        probe: probeOf({ completion: TYPED })
      })
    ).toBeNull();
  });

  it("fills nothing when no folder matches", () => {
    expect(
      nextSegmentCompletion({
        typed: TYPED,
        probe: probeOf({ completion: "" })
      })
    ).toBeNull();
  });

  it("never trusts a probe of text typed earlier", () => {
    expect(
      nextSegmentCompletion({
        typed: TYPED,
        probe: probeOf({
          path: String.raw`C:\repositories\p`,
          completion: String.raw`C:\repositories\pad`
        })
      })
    ).toBeNull();
  });
});
