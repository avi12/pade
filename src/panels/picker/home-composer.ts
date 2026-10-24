import type { OpenTarget } from "@/lib/types";
import { FirstPrompt, parseInput, ProjectName } from "@/lib/validate";

/** What the home composer does when Start is pressed: open the chosen project
 *  with the typed prompt and picked agent, create (save) a new project under
 *  its root first, or nothing when there is nothing actionable yet. */
export type HomeStart =
  | {
    kind: "open";
    target: OpenTarget;
  }
  | {
    kind: "create";
    root: string;
    name: string;
    prompt: string;
    agent: string | null;
  }
  | {
    kind: "none";
  };

/** Resolve the composer's state to what Start should do. An existing project
 *  always wins outright; a new one needs both a root and a valid name. A blank
 *  prompt is fine either way - it just opens without seeding one. */
export function resolveHomeStart({ selectedPath, isCreating, root, rawName, rawPrompt, agent }: {
  selectedPath: string;
  isCreating: boolean;
  root: string;
  rawName: string;
  rawPrompt: string;
  agent: string | null;
}): HomeStart {
  const prompt = parseInput({
    schema: FirstPrompt,
    raw: rawPrompt
  }) ?? "";
  if (isCreating) {
    const name = parseInput({
      schema: ProjectName,
      raw: rawName
    });
    if (!root || !name) {
      return { kind: "none" };
    }

    return {
      kind: "create",
      root,
      name,
      prompt,
      agent
    };
  }

  if (!selectedPath) {
    return { kind: "none" };
  }

  return {
    kind: "open",
    target: {
      path: selectedPath,
      initialPrompt: prompt || undefined,
      agent: agent ?? undefined
    }
  };
}
