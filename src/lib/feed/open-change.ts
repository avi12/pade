// Opening a Change Feed card's file: a binary file (an .exe, an archive) means
// nothing in a text editor, so it is shown selected in its folder instead; every
// other file opens in the project's ranked windowed editor, at a line.

import { isBinaryPath } from "@/lib/feed/preview/preview";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { Ide } from "@/lib/types";

/** The verb on a card's open button - the one source its tooltip and its
 *  enabled state read, so both follow the same editor-or-folder decision. Empty
 *  when there is nowhere to open it. */
export function openAction({ path, editor }: {
  path: string;
  editor: Ide | undefined;
}) {
  if (isBinaryPath(path)) {
    return "Show in folder";
  }

  return editor ? `Open in ${editor.label}` : "";
}

export async function openChangedFile({ path, line, project, editor }: {
  path: string;
  line?: number;
  project: string;
  editor: Ide | undefined;
}) {
  if (isBinaryPath(path)) {
    await sendMessage(Command.revealInExplorer, { path });
    return;
  }

  if (editor) {
    await sendMessage(Command.ideOpenFile, {
      command: editor.command,
      project,
      file: path,
      line
    });
  }
}
