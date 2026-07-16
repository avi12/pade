import {
  FirstPrompt,
  FolderPath,
  nameError,
  parseInput,
  ProjectName,
  RestoreQuery,
  SessionName
} from "@/lib/validate";
import { describe, expect, it } from "vitest";

describe("parseInput", () => {
  it("returns the trimmed value when the schema accepts", () => {
    const parsed = parseInput({
      schema: RestoreQuery,
      raw: "  hello  "
    });

    expect(parsed).toBe("hello");
  });

  it("returns null when the schema rejects", () => {
    const parsed = parseInput({
      schema: RestoreQuery,
      raw: "   "
    });

    expect(parsed).toBeNull();
  });

  it("returns null for a non-string value", () => {
    const parsed = parseInput({
      schema: RestoreQuery,
      raw: 42
    });

    expect(parsed).toBeNull();
  });
});

describe("ProjectName", () => {
  it("accepts a kebab-case name", () => {
    expect(ProjectName.safeParse("brave-otter").success).toBe(true);
  });

  it("trims surrounding whitespace", () => {
    const parsed = parseInput({
      schema: ProjectName,
      raw: "  demo  "
    });

    expect(parsed).toBe("demo");
  });

  it("rejects path separators", () => {
    expect(ProjectName.safeParse("a/b").success).toBe(false);
    expect(ProjectName.safeParse("a\\b").success).toBe(false);
  });

  it("rejects Windows-reserved characters", () => {
    expect(ProjectName.safeParse("what?").success).toBe(false);
    expect(ProjectName.safeParse("a:b").success).toBe(false);
    expect(ProjectName.safeParse(`say "hi"`).success).toBe(false);
  });

  it("caps the length at 100 characters", () => {
    expect(ProjectName.safeParse("x".repeat(100)).success).toBe(true);
    expect(ProjectName.safeParse("x".repeat(101)).success).toBe(false);
  });
});

describe("nameError", () => {
  it("stays quiet while the field is empty", () => {
    expect(nameError("")).toBeNull();
    expect(nameError("   ")).toBeNull();
  });

  it("stays quiet for a valid name", () => {
    expect(nameError("brave-otter")).toBeNull();
  });

  it("surfaces the schema's own message for an invalid name", () => {
    expect(nameError("a/b")).toBe("Name can't contain path characters.");
  });
});

describe("FirstPrompt", () => {
  it("allows an empty prompt", () => {
    expect(FirstPrompt.safeParse("").success).toBe(true);
  });

  it("caps the length at 10000 characters", () => {
    expect(FirstPrompt.safeParse("x".repeat(10_001)).success).toBe(false);
  });
});

describe("SessionName", () => {
  it("requires at least one non-whitespace character", () => {
    expect(SessionName.safeParse("").success).toBe(false);
    expect(SessionName.safeParse("   ").success).toBe(false);
  });

  it("trims surrounding whitespace", () => {
    const parsed = parseInput({
      schema: SessionName,
      raw: "  planning  "
    });

    expect(parsed).toBe("planning");
  });

  it("caps the length at 60 characters", () => {
    expect(SessionName.safeParse("x".repeat(60)).success).toBe(true);
    expect(SessionName.safeParse("x".repeat(61)).success).toBe(false);
  });
});

describe("length caps", () => {
  it("caps RestoreQuery at 200 characters", () => {
    expect(RestoreQuery.safeParse("x".repeat(200)).success).toBe(true);
    expect(RestoreQuery.safeParse("x".repeat(201)).success).toBe(false);
  });

  it("caps FolderPath at 4096 characters", () => {
    expect(FolderPath.safeParse("x".repeat(4096)).success).toBe(true);
    expect(FolderPath.safeParse("x".repeat(4097)).success).toBe(false);
  });
});
