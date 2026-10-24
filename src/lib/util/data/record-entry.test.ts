import { withRecordEntry } from "@/lib/util/data/record-entry";
import { describe, expect, it } from "vitest";

describe("withRecordEntry", () => {
  it("sets the key on a copy, leaving the original untouched", () => {
    const record = { a: 1 };
    expect(
      withRecordEntry({
        record,
        key: "b",
        value: 2
      })
    ).toEqual({
      a: 1,
      b: 2
    });
    expect(record).toEqual({ a: 1 });
  });

  it("drops the key when the value is undefined", () => {
    expect(
      withRecordEntry({
        record: {
          a: 1,
          b: 2
        },
        key: "a",
        value: undefined
      })
    ).toEqual({ b: 2 });
  });

  it("starts from an empty record when there is none", () => {
    expect(
      withRecordEntry({
        record: undefined,
        key: "a",
        value: true
      })
    ).toEqual({ a: true });
  });
});
