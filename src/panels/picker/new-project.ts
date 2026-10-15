import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { OpenTarget } from "@/lib/types";
import { childPath } from "@/lib/util/paths";
import { FirstPrompt, parseInput, ProjectName } from "@/lib/validate";

/** What the disk holds at a would-be project path, tagged with that path so an
 *  out-of-order answer never gates the text currently typed. */
export interface CreateTarget {
  path: string;
  isDir: boolean;
  isFile: boolean;
}

interface NewProjectHandoff {
  /** The agent to launch in the new workspace, if one was picked. */
  agent: string | null;
  onopen: (target: OpenTarget) => void;
}

/** The open request for a just-created workspace, carrying the optional first
 *  prompt and the picked agent. */
function createdTarget({ path, prompt, agent }: {
  path: string;
  prompt: string;
  agent: string | null;
}) {
  return {
    path,
    initialPrompt: prompt || undefined,
    agent: agent ?? undefined,
    created: true
  } satisfies OpenTarget;
}

async function openTemp({ prompt, agent, onopen }: NewProjectHandoff & { prompt: string }) {
  const path = await sendMessage(Command.workspaceTemp);
  onopen(
    createdTarget({
      path,
      prompt,
      agent
    })
  );
}

/** Start a throwaway temp workspace with the typed first prompt - a no-op when
 *  the prompt fails validation. */
export async function startTempWorkspace({ rawPrompt, agent, onopen }: NewProjectHandoff & { rawPrompt: string }) {
  const prompt = parseInput({
    schema: FirstPrompt,
    raw: rawPrompt
  });
  if (prompt === null) {
    return;
  }

  await openTemp({
    prompt,
    agent,
    onopen
  });
}

/** Submit the New-project form: a blank name falls through to a temp workspace;
 *  a typed one is created under `root`. The create is preceded by an
 *  authoritative re-probe - a fast Enter can outrun the debounced live check,
 *  and the backend's create_dir_all would silently "create" an existing folder -
 *  whose answer goes to `onprobe` so the inline reason shows if it blocks. */
export async function submitNewProject({ root, rawName, rawPrompt, agent, onopen, onprobe }: NewProjectHandoff & {
  root: string;
  rawName: string;
  rawPrompt: string;
  onprobe: (target: CreateTarget) => void;
}) {
  const prompt = parseInput({
    schema: FirstPrompt,
    raw: rawPrompt
  });
  if (prompt === null) {
    return;
  }

  const isNameBlank = rawName.trim().length === 0;
  if (isNameBlank) {
    await openTemp({
      prompt,
      agent,
      onopen
    });
    return;
  }

  const name = parseInput({
    schema: ProjectName,
    raw: rawName
  });
  if (!root || !name) {
    return;
  }

  const path = childPath({
    parent: root,
    name
  });
  const probe = await sendMessage(Command.workspaceProbePath, { path });
  onprobe({
    path,
    isDir: probe.isDir,
    isFile: probe.isFile
  });

  if (probe.isDir || probe.isFile) {
    return;
  }

  const created = await sendMessage(Command.workspaceCreate, {
    root,
    name
  });
  onopen(
    createdTarget({
      path: created,
      prompt,
      agent
    })
  );
}
