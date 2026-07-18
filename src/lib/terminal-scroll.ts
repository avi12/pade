// Turn a stream of mouse-wheel deltas into whole scroll "notches".
//
// A wheel — and far more so a trackpad — fires many small delta events per
// gesture, in one of three units (pixels, whole lines, or whole pages). The
// terminal forwards each notch to a fullscreen agent as one PageUp/PageDown, and
// Claude Code scrolls its transcript a half-page per press — already a large jump
// — so a naive one-keypress-per-event mapping would fly through the entire
// conversation on a single flick. This accumulates the sub-notch remainder across
// calls and clamps how big a jump any one event may produce.

// A typical mouse-wheel detent reports ~120px (Chromium's WHEEL_DELTA), so one
// detent maps to one notch. A trackpad's smaller deltas accumulate toward it.
const PIXELS_PER_NOTCH = 120;
// DOM_DELTA_LINE deltas are counted in text rows; treat one row as this many
// pixels so line-mode wheels land near the same feel as pixel-mode ones.
const PIXELS_PER_WHEEL_LINE = 16;
// One physical gesture must never leap more than this, however large a single
// delta (a fling, or a page-mode wheel) claims to be.
const MAX_NOTCHES_PER_EVENT = 3;

// The three units a WheelEvent's deltaY can carry (its `deltaMode`). Named here so
// the accumulator stays a pure numeric function with no dependency on the DOM
// WheelEvent global — which keeps it unit-testable off a browser.
const WheelDeltaMode = {
  Pixel: 0,
  Line: 1,
  Page: 2
} as const;

export interface WheelScroll {
  /** Whole notches this event yields — negative scrolls up (toward earlier output). */
  notches: number;
  /** Sub-notch pixel remainder to pass back into the next call. */
  carry: number;
}

// Normalize a wheel delta to pixels, whatever unit it arrived in.
function wheelDeltaPixels({
  deltaY,
  deltaMode
}: {
  deltaY: number;
  deltaMode: number;
}): number {
  if (deltaMode === WheelDeltaMode.Line) {
    return deltaY * PIXELS_PER_WHEEL_LINE;
  }

  if (deltaMode === WheelDeltaMode.Page) {
    return deltaY * PIXELS_PER_NOTCH;
  }

  return deltaY;
}

export function accumulateWheelNotches({
  deltaY,
  deltaMode,
  carry
}: {
  deltaY: number;
  deltaMode: number;
  carry: number;
}): WheelScroll {
  const total = carry + wheelDeltaPixels({
    deltaY,
    deltaMode
  });
  const whole = Math.trunc(total / PIXELS_PER_NOTCH);
  const notches = Math.max(-MAX_NOTCHES_PER_EVENT, Math.min(MAX_NOTCHES_PER_EVENT, whole));
  return {
    notches,
    carry: total - whole * PIXELS_PER_NOTCH
  };
}
