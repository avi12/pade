// The `KeyboardEvent.key` values the app's keyboard handling reacts to - one
// home so the roving lists, the shortcut matchers, the drag's cancel key and
// every component key handler compare against a member, never a bare string.
export const Key = {
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  Backspace: "Backspace",
  End: "End",
  Enter: "Enter",
  Escape: "Escape",
  F4: "F4",
  Home: "Home",
  Space: " ",
  Tab: "Tab"
} as const;
