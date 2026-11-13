/** The two states a popover's `toggle` event reports in `newState`. */
export const ToggleState = {
  OPEN: "open",
  CLOSED: "closed"
} as const;

/** The `newState` of a popover's `toggle` event, or undefined for any other event. */
export function toggleStateOf(event: Event) {
  return event instanceof ToggleEvent ? event.newState : undefined;
}
