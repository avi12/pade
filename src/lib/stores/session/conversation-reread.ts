// Which of the listed projects re-read their recorded conversations when an
// agent's store reports a change: the one it names, or every listed one when
// the store names no project.

import { normalizePath } from "@/lib/util/paths";

export function projectsToReread({ listed, changedProject }: {
  listed: readonly string[];
  changedProject: string | null;
}) {
  if (changedProject === null) {
    return listed;
  }

  const changed = normalizePath(changedProject);
  return listed.filter(project => normalizePath(project) === changed);
}
