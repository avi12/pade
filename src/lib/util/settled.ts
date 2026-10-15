// A best-effort read: the value a promise resolves to, or a stated fallback
// when it rejects. The fallback IS the recovery, so the call site says what a
// failure looks like instead of hiding it in a `.catch` chain.

export async function settledValue<T>({ promise, fallback }: {
  promise: Promise<T>;
  fallback: T;
}) {
  try {
    return await promise;
  } catch {
    return fallback;
  }
}
