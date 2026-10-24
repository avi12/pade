// A window holds sessions from several projects at once. These pure rules say
// which project is in front (the window's project context follows the active
// session) and which sessions belong to a project - the tab strip shows only
// the front project's, while every session stays mounted.

import type { AgentSession } from "@/lib/types";
import { normalizePath } from "@/lib/util/paths";

/** Whether two project paths name the same folder. */
export function isSameProject(left: string, right: string) {
  return normalizePath(left) === normalizePath(right);
}

/** The project in front: the active session's, else the one the window was
 *  last pointed at (entered, or left by its last session). */
export function frontProject({ activeSession, fallbackProject }: {
  activeSession: Pick<AgentSession, "project"> | undefined;
  fallbackProject: string;
}) {
  return activeSession?.project ?? fallbackProject;
}

/** The sessions of one project, in the window's order. */
export function sessionsOfProject<Session extends Pick<AgentSession, "project">>({ sessions, project }: {
  sessions: readonly Session[];
  project: string;
}) {
  return sessions.filter(session => isSameProject(session.project, project));
}

/** The session to bring forward when a project with live sessions is picked:
 *  the one it last had in front when that is still live, else its newest. */
export function sessionToFocus({ sessions, project, lastActiveId }: {
  sessions: readonly Pick<AgentSession, "id" | "project">[];
  project: string;
  lastActiveId: string | undefined;
}) {
  const projectSessions = sessionsOfProject({
    sessions,
    project
  });
  const lastActive = projectSessions.find(session => session.id === lastActiveId);
  return (lastActive ?? projectSessions.at(-1))?.id;
}
