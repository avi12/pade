import { StartMode } from "@/lib/types";

/** Where a launch with no project lands. The homescreen (the project picker)
 *  is the default; starting straight in a temp workspace happens only when
 *  the user chose it in the picker's "On launch" section. */
export function resolvedStartMode({ startMode }: {
  startMode: StartMode | null | undefined;
}) {
  return startMode ?? StartMode.enum.picker;
}
