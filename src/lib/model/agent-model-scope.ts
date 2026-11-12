// Where a new session's starting model is decided (one pure resolution, no UI,
// no store), the same shape as the per-project AI-naming choice:
//
//   workspace default  (prefs.agentModelProjects[project][agent])  - Config, "This workspace"
//   PADE-wide default  (prefs.agentModelGlobal[agent])             - Config, "Every workspace"
//   the agent's own    (no entry)                                  - its saved default
//
// Absence is the only way back to the next level down, so clearing writes the
// key away rather than freezing whatever the agent happened to run.

import type { ModelChoice } from "@/lib/types/agent";
import type { Prefs } from "@/lib/types/settings";
import { withRecordEntry } from "@/lib/util/data/record-entry";

/** The workspace's own starting model for `agent`, or `undefined` to follow
 *  the PADE-wide one. */
export function projectModelDefault({ prefs, project, agent }: {
  prefs: Prefs;
  project: string;
  agent: string;
}) {
  return prefs.agentModelProjects?.[project]?.[agent];
}

/** Every PADE workspace's starting model for `agent`, or `undefined` to start
 *  on the agent's own default. */
export function globalModelDefault({ prefs, agent }: {
  prefs: Prefs;
  agent: string;
}) {
  return prefs.agentModelGlobal?.[agent];
}

/** What a new session starts on: an explicit choice (a handoff successor
 *  keeping its predecessor's) wins, else the workspace's default for the
 *  agent, else every workspace's. */
export function startingModelChoice({ explicit, prefs, project, agent }: {
  explicit: ModelChoice | undefined;
  prefs: Prefs;
  project: string;
  agent: string;
}) {
  return explicit ?? projectModelDefault({
    prefs,
    project,
    agent
  }) ?? globalModelDefault({
    prefs,
    agent
  });
}

/** `choice`, or undefined when it picks nothing - cleared back a level. */
function keptChoice(choice: ModelChoice | undefined) {
  const isCleared = choice === undefined || (choice.model === undefined && choice.effort === undefined);
  return isCleared ? undefined : choice;
}

/** The `agentModelProjects` record with this workspace's `agent` default set to
 *  `choice`, or dropped when `choice` picks nothing (and the workspace dropped
 *  once it has no defaults left). */
export function withProjectModelDefault({ prefs, project, agent, choice }: {
  prefs: Prefs;
  project: string;
  agent: string;
  choice: ModelChoice | undefined;
}) {
  const projectDefaults = withRecordEntry({
    record: prefs.agentModelProjects?.[project],
    key: agent,
    value: keptChoice(choice)
  });
  const hasDefaults = Object.keys(projectDefaults).length > 0;
  return withRecordEntry({
    record: prefs.agentModelProjects,
    key: project,
    value: hasDefaults ? projectDefaults : undefined
  });
}

/** The `agentModelGlobal` record with `agent`'s default set to `choice`, or
 *  dropped when `choice` picks nothing. */
export function withGlobalModelDefault({ prefs, agent, choice }: {
  prefs: Prefs;
  agent: string;
  choice: ModelChoice | undefined;
}) {
  return withRecordEntry({
    record: prefs.agentModelGlobal,
    key: agent,
    value: keptChoice(choice)
  });
}
