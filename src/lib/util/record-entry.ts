/** A copy of `record` with `key` set to `value`, or with `key` dropped when
 *  `value` is `undefined`. The per-project prefs maps use absence as "follow the
 *  inherited default", so clearing must remove the key rather than store a value. */
export function withRecordEntry<T>({ record, key, value }: {
  record: Readonly<Record<string, T>> | null | undefined;
  key: string;
  value: T | undefined;
}) {
  const next: Record<string, T> = { ...record };
  if (value === undefined) {
    delete next[key];
    return next;
  }

  next[key] = value;
  return next;
}
