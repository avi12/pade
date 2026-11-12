// The fixed clock and the seeded generator every fixture draws from - what keeps a
// rerun byte-identical.

export const NOW = Date.UTC(2026, 9, 15, 12);

export const MINUTE = 60_000;

export const HOUR = 60 * MINUTE;

export const DAY = 24 * HOUR;

const SEED = 0x9ace_2026;

/** A burn rate in the formula's units, from percent per hour. */
export function perHour(percentage: number) {
  return percentage / HOUR;
}

/** A small seeded generator (mulberry32), so a rerun draws the same scatter. */
function seededRandom(seed: number) {
  let state = seed;
  return () => {
    state = (state + 0x6d2b_79f5) | 0;
    let mixed = Math.imul(state ^ (state >>> 15), 1 | state);
    mixed = (mixed + Math.imul(mixed ^ (mixed >>> 7), 61 | mixed)) ^ mixed;
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4_294_967_296;
  };
}

export const random = seededRandom(SEED);

export function pick<Item>(items: readonly Item[]) {
  const [item] = items.slice(Math.floor(random() * items.length));
  if (item === undefined) {
    throw new Error("Nothing to pick from");
  }

  return item;
}

/** A uniform draw rounded to a step, so the JSON stays short. */
export function between({ low, high, step }: {
  low: number;
  high: number;
  step: number;
}) {
  return Math.round((low + random() * (high - low)) / step) * step;
}
