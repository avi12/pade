import { EMPTY_PATH_PROBE } from "@/lib/types";
import type { PathProbe } from "@/lib/types";
import { assessRootPath } from "@/panels/picker/root-path";
import { describe, expect, it } from "vitest";

const TYPED = "C:\\repositories\\avi";

function assess({ typed = TYPED, roots = [], probed = TYPED, result = {} }: {
  typed?: string;
  roots?: string[];
  probed?: string;
  result?: Partial<PathProbe>;
}) {
  return assessRootPath({
    typed,
    roots,
    probe: {
      path: probed,
      result: {
        ...EMPTY_PATH_PROBE,
        ...result
      }
    }
  });
}

describe("assessRootPath", () => {
  it("says nothing for an empty field", () => {
    const assessment = assess({
      typed: " ",
      probed: ""
    });
    expect(assessment.status).toBeNull();
    expect(assessment.addablePath).toBeNull();
  });

  it("waits for the probe to describe the typed path", () => {
    const assessment = assess({ probed: "C:\\repositories" });
    expect(assessment.status?.text).toBe("Checking…");
    expect(assessment.addablePath).toBeNull();
  });

  it("adds an existing folder as typed", () => {
    const assessment = assess({
      result: {
        isDir: true
      }
    });
    expect(assessment.status?.tone).toBe("ok");
    expect(assessment.addablePath).toBe(TYPED);
    expect(assessment.willCreate).toBe(false);
  });

  it("treats a root differing only by case or a trailing separator as the same", () => {
    const assessment = assess({
      roots: ["c:\\Repositories\\AVI\\"],
      result: {
        isDir: true
      }
    });
    expect(assessment.status?.tone).toBe("warning");
    expect(assessment.addablePath).toBeNull();
  });

  it("rejects a file", () => {
    const assessment = assess({
      result: {
        isFile: true
      }
    });
    expect(assessment.status?.text).toBe("That path is a file, not a folder");
    expect(assessment.addablePath).toBeNull();
  });

  it("creates a missing folder whose parent exists", () => {
    const assessment = assess({
      result: {
        parentExists: true
      }
    });
    expect(assessment.status?.tone).toBe("primary");
    expect(assessment.canCreate).toBe(true);
    expect(assessment.willCreate).toBe(true);
    expect(assessment.addablePath).toBe(TYPED);
  });

  it("rejects a location whose parent doesn't exist", () => {
    const assessment = assess({});
    expect(assessment.status?.tone).toBe("critical");
    expect(assessment.addablePath).toBeNull();
  });
});
