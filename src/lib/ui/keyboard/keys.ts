// The `KeyboardEvent.key` values the app's keyboard handling reacts to - one
// home so the roving lists, the shortcut matchers, the drag's cancel key and
// every component key handler compare against a member, never a bare string.
export const Key = {
  ARROW_UP: "ArrowUp",
  ARROW_DOWN: "ArrowDown",
  ARROW_LEFT: "ArrowLeft",
  ARROW_RIGHT: "ArrowRight",
  BACKSPACE: "Backspace",
  END: "End",
  ENTER: "Enter",
  ESCAPE: "Escape",
  F4: "F4",
  HOME: "Home",
  SPACE: " ",
  TAB: "Tab"
} as const;
