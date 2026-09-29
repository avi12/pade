// Where "name session tabs with AI" is decided (SoC: one pure resolution, no UI,
// no store). Two scopes, the same shape the app already uses for the default
// agent and the per-project editor pick:
//
//   project override  (prefs.aiSessionNamingProjects[path])   — the Config panel
//   global default    (prefs.aiSessionNaming)                 — the project picker
//   built-in          (off)
//
// Absence is meaningful at both levels and is the ONLY way back: a project with
// no entry follows the global default, and a global default that was never set
// is off. So the Config control writes `undefined` to revert rather than writing
// the inherited value, which would silently freeze this project at whatever the
// global happened to be that day.

import type { Prefs } from "@/lib/types";

/** Naming is opt-in — it spends agent turns, so nothing names until asked. */
const AI_NAMING_DEFAULT = false;

/** This project's explicit choice, or `undefined` when it follows the global. */
export function projectNamingChoice({ prefs, project }: {
  prefs: Prefs;
  project: string;
}): boolean | undefined {
  return prefs.aiSessionNamingProjects?.[project] ?? undefined;
}

/** The global default every project falls back to. */
export function globalNaming(prefs: Prefs): boolean {
  return prefs.aiSessionNaming ?? AI_NAMING_DEFAULT;
}

/** Whether AI session naming is on for this project — the one resolution of
 *  project override → global default → off. Every caller reads it through here,
 *  so the Config panel's control, the picker's checkbox and what the tabs
 *  actually do can never disagree. */
export function aiNamingEnabled({ prefs, project }: {
  prefs: Prefs;
  project: string;
}): boolean {
  return projectNamingChoice({
    prefs,
    project
  }) ?? globalNaming(prefs);
}

/** The `aiSessionNamingProjects` record with this project set to `choice` — or
 *  with its key dropped when `choice` is `undefined`, which is how a project
 *  goes back to following the global default. */
export function withProjectNamingChoice({ prefs, project, choice }: {
  prefs: Prefs;
  project: string;
  choice: boolean | undefined;
}): Record<string, boolean> {
  const next = { ...prefs.aiSessionNamingProjects };
  if (choice === undefined) {
    delete next[project];
    return next;
  }

  next[project] = choice;
  return next;
}
