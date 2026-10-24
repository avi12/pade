// The save card's form logic (SaveWorkspaceCard): which label is worth
// prefilling as the project name, and whether that name is already taken in
// each root the temp workspace could be saved into.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { childPath } from "@/lib/util/paths";
import { parseInput, ProjectName } from "@/lib/validate";

// The auto-generated folder name a temp workspace starts with - never worth
// prefilling, unlike a friendly label.
const TEMP_FOLDER_NAME = /^temp-\d+$/;

/** One root's answer to "does `<root>/<name>` already exist?", tagged with the
 *  name it described so an out-of-order reply never gates the current text. */
export interface SaveProbe {
  name: string;
  root: string;
  exists: boolean;
}

/** The workspace's label as a ready project name, or null when it is not a
 *  valid name or only the raw temp-<stamp> folder name. */
export function prefillableName(label: string) {
  const friendlyLabel = parseInput({
    schema: ProjectName,
    raw: label
  });
  if (friendlyLabel === null || TEMP_FOLDER_NAME.test(friendlyLabel)) {
    return null;
  }

  return friendlyLabel;
}

/** Probe `<root>/<name>` in every root. `workspace_rename` does an `fs::rename`,
 *  which on Windows fails with a raw "os error 32/183" if the target already
 *  exists, so a colliding name is caught up front instead. */
export async function probeSaveTargets({ name, roots }: {
  name: string;
  roots: readonly string[];
}) {
  return await Promise.all(
    roots.map(async root => {
      try {
        const probe = await sendMessage(Command.workspaceProbePath, {
          path: childPath({
            parent: root,
            name
          })
        });
        return {
          name,
          root,
          exists: probe.isDir || probe.isFile
        } satisfies SaveProbe;
      } catch {
        // An unreachable probe leaves the plain save path - the backend
        // still validates the final rename.
        return {
          name,
          root,
          exists: false
        } satisfies SaveProbe;
      }
    })
  );
}

/** Whether `name` is known to exist in `root` already. */
export function isNameTakenIn({ probes, root, name }: {
  probes: readonly SaveProbe[];
  root: string;
  name: string | null;
}) {
  return probes.some(probe => probe.root === root && probe.name === name && probe.exists);
}
