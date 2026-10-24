// Where usage pacing's settings are decided (one pure resolution, no UI, no
// store), per window, the same shape as the per-project AI-naming choice:
//
//   workspace override  (prefs.pacingProjects[project][window])  - Config, "This workspace"
//   global              (prefs.pacing[window])                   - Config, "Global"
//   built-in            (DEFAULT_PACING[window])
//
// Absence is the only way back: a workspace window with no entry follows the
// global, so "Use global" drops the key rather than freezing a copy of it.

import { PacedWindowKind } from "@/lib/types";
import type { PacingByWindow, PacingSettings, Prefs } from "@/lib/types";
import { DEFAULT_PACING } from "@/lib/usage/pacing-policy";
import { withRecordEntry } from "@/lib/util/record-entry";

/** The global settings every workspace falls back to, per window. */
export function globalPacing(prefs: Prefs) {
  const pacing: PacingByWindow = { ...DEFAULT_PACING };
  for (const kind of PacedWindowKind.options) {
    pacing[kind] = prefs.pacing?.[kind] ?? DEFAULT_PACING[kind];
  }

  return pacing;
}

/** This workspace's override for one window, or `undefined` when it follows
 *  the global. */
export function projectWindowPacing({ prefs, project, kind }: {
  prefs: Prefs;
  project: string;
  kind: PacedWindowKind;
}) {
  return prefs.pacingProjects?.[project]?.[kind];
}

/** The settings pacing runs on in this workspace - the one resolution of
 *  workspace override → global → built-in, so the Config panel, the tab badge
 *  and the pacing loop can never disagree. */
export function resolvedPacing({ prefs, project }: {
  prefs: Prefs;
  project: string;
}) {
  const pacing = globalPacing(prefs);
  for (const kind of PacedWindowKind.options) {
    pacing[kind] = projectWindowPacing({
      prefs,
      project,
      kind
    }) ?? pacing[kind];
  }

  return pacing;
}

/** The global `pacing` with one window set. */
export function withGlobalWindowPacing({ prefs, kind, settings }: {
  prefs: Prefs;
  kind: PacedWindowKind;
  settings: PacingSettings;
}) {
  return {
    ...globalPacing(prefs),
    [kind]: settings
  };
}

/** The `pacingProjects` record with this workspace's window set to `settings`,
 *  or dropped when `settings` is `undefined` (and the workspace dropped once it
 *  overrides nothing). */
export function withProjectWindowPacing({ prefs, project, kind, settings }: {
  prefs: Prefs;
  project: string;
  kind: PacedWindowKind;
  settings: PacingSettings | undefined;
}) {
  const overrides = withRecordEntry({
    record: prefs.pacingProjects?.[project],
    key: kind,
    value: settings
  });
  const hasOverrides = Object.keys(overrides).length > 0;
  return withRecordEntry({
    record: prefs.pacingProjects,
    key: project,
    value: hasOverrides ? overrides : undefined
  });
}
