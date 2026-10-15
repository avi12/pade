import { UI_SCALE_MAXIMUM, UI_SCALE_MINIMUM } from "@/lib/settings/prefs-bounds";

/** The scale is stored on a hundredths grid (1.05, not 1.0500000000000003). */
const SCALE_GRID = 100;

/** The UI scale one stepper press lands on: held inside the bounds, and rounded
 *  to the step grid so accumulated float drift never leaks into the pref. */
export function steppedUiScale({ scale, delta }: {
  scale: number;
  delta: number;
}) {
  const clamped = Math.min(UI_SCALE_MAXIMUM, Math.max(UI_SCALE_MINIMUM, scale + delta));
  return Math.round(clamped * SCALE_GRID) / SCALE_GRID;
}
