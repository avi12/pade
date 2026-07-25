// Single authoritative home for the numeric bounds of user preferences. Both the
// zod schema that validates the persisted value (lib/types) and the UI that lets
// the user change it (ConfigPanel stepper, the side-panel drag/clamp in App)
// read these — so a bound can never say one thing in the schema and another in
// the control. Kept as a plain leaf module (no runes, no type imports) so the
// schema in lib/types can import it without a cycle.

/** Side-panel width (Feed/Git/Tasks/Config) in pixels. Default matches the design
 *  mockup's `panelW:380` and its double-click reset. */
export const SIDE_PANEL_DEFAULT_WIDTH = 380;
/** Smallest usable side-panel width. */
export const SIDE_PANEL_MIN_WIDTH = 280;
/** Hard upper bound the persisted drag target is validated against. */
export const SIDE_PANEL_MAX_WIDTH = 1200;
/** The live layout additionally caps the panel at this fraction of the window so
 *  the terminal is never swallowed — the ceiling the keyboard clamp and the CSS
 *  clamp both enforce. */
export const SIDE_PANEL_MAX_FRACTION = 0.6;

/** UI + terminal zoom factor. Absent pref = 1.0. */
export const UI_SCALE_MIN = 0.85;
export const UI_SCALE_MAX = 1.3;
export const UI_SCALE_STEP = 0.05;
