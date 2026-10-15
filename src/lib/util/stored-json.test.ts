import { readStoredJson, writeStoredJson } from "@/lib/util/stored-json";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";
import { z } from "zod";

const backing = new Map<string, string>();
const Point = z.object({
  x: z.number(),
  y: z.number()
});

beforeEach(() => {
  backing.clear();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => backing.get(key) ?? null,
    setItem(key: string, value: string) {
      backing.set(key, value);
    }
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("stored JSON", () => {
  it("round-trips a value that matches the schema", () => {
    writeStoredJson({
      key: "point",
      value: {
        x: 1,
        y: 2
      }
    });
    expect(
      readStoredJson({
        key: "point",
        schema: Point
      })
    ).toEqual({
      x: 1,
      y: 2
    });
  });

  it("reads nothing when the key is absent", () => {
    expect(
      readStoredJson({
        key: "point",
        schema: Point
      })
    ).toBeNull();
  });

  it("reads nothing for a payload that isn't JSON", () => {
    backing.set("point", "{ not json");
    expect(
      readStoredJson({
        key: "point",
        schema: Point
      })
    ).toBeNull();
  });

  it("reads nothing for JSON of the wrong shape", () => {
    backing.set("point", JSON.stringify({ x: "1" }));
    expect(
      readStoredJson({
        key: "point",
        schema: Point
      })
    ).toBeNull();
  });
});
