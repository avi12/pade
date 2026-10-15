// A JSON value kept in localStorage across a reload of its window. Storage is a
// trust boundary like any other - a stale, hand-edited or half-written payload
// must read as "nothing stored" - so every read goes through a zod schema.

import type { z } from "zod";

function decodedJson(raw: string) {
  try {
    const decoded: unknown = JSON.parse(raw);
    return { decoded };
  } catch {
    return null;
  }
}

/** The value stored under `key`, or `null` when it is absent, not JSON, or not
 *  the shape `schema` describes. */
export function readStoredJson<T>({ key, schema }: {
  key: string;
  schema: z.ZodType<T>;
}) {
  const raw = localStorage.getItem(key);
  if (raw === null) {
    return null;
  }

  const json = decodedJson(raw);
  if (json === null) {
    return null;
  }

  const parsed = schema.safeParse(json.decoded);
  return parsed.success ? parsed.data : null;
}

export function writeStoredJson<T>({ key, value }: {
  key: string;
  value: T;
}) {
  localStorage.setItem(key, JSON.stringify(value));
}
