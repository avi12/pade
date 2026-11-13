// The workspace's side panels, defined once: the top bar's segmented control
// shows each one's short label (full name in a tooltip) and the shared aside
// header shows the full label. Count + refresh are published per panel via the
// sidePanel store.

/** A closed set of panel ids; the open panel is one of these or `null`. */
export const Side = {
  FEED: "feed",
  VCS: "vcs",
  TASKS: "tasks",
  CONFIG: "config"
} as const;
export type Side = (typeof Side)[keyof typeof Side];

export const SIDE_PANEL_TABS = [
  {
    id: Side.FEED,
    icon: "activity",
    short: "Feed",
    label: "Change Feed"
  },
  {
    id: Side.VCS,
    icon: "branch",
    short: "Git",
    label: "Version control"
  },
  {
    id: Side.TASKS,
    icon: "checks",
    short: "Tasks",
    label: "Tasks"
  },
  {
    id: Side.CONFIG,
    icon: "sliders",
    short: "Config",
    label: "Agent config"
  }
] as const;
