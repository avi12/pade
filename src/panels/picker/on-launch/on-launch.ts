// The choices the picker's "On launch" section offers, each with the label
// its pill prints - the section renders them, it never spells them out.

import { StartMode, TEMP_CLEANUP_DAY_CHOICES } from "@/lib/types";

/** One pill of a single-choice row: the value it picks and its label. */
export interface LabelledChoice<Value> {
  value: Value;
  label: string;
}

/** What to open with no project. */
export const START_MODE_CHOICES: readonly LabelledChoice<StartMode>[] = [
  {
    value: StartMode.enum.temp,
    label: "Temp workspace"
  },
  {
    value: StartMode.enum.picker,
    label: "This picker"
  }
];

function tempCleanupLabel(days: number | null) {
  if (days === null) {
    return "Off";
  }

  return `${days} days`;
}

/** How long an unused temp workspace lives - or Off, never deleted. */
export const TEMP_CLEANUP_CHOICES: readonly LabelledChoice<number | null>[] = TEMP_CLEANUP_DAY_CHOICES.map(days => ({
  value: days,
  label: tempCleanupLabel(days)
}));
