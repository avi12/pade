import type { OpenTarget } from "@/lib/types";
import { FirstPrompt, parseInput, ProjectName } from "@/lib/validate";

/** What the composer's project menu has picked: a project that exists, a new
 *  one to name under a root, or a throwaway workspace. */
export const ProjectChoice = {
  Existing: "existing",
  New: "new",
  Temp: "temp"
} as const;
export type ProjectChoice = typeof ProjectChoice[keyof typeof ProjectChoice];

/** What the home composer does when Start is pressed: open the chosen project
 *  with the typed prompt and picked agent, create (save) a new project under
 *  its root first, start a throwaway workspace, or nothing when there is
 *  nothing actionable yet. */
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
    kind: "temp";
  }
  | {
    kind: "none";
  };

/** Resolve the composer's state to what Start should do. An existing project
 *  always wins outright; a new one needs both a root and a valid name; a
 *  throwaway workspace needs nothing more. A blank prompt is fine every way -
 *  it just opens without seeding one. */
export function resolveHomeStart({ selectedPath, choice, root, rawName, rawPrompt, agent }: {
  selectedPath: string;
  choice: ProjectChoice;
  root: string;
  rawName: string;
  rawPrompt: string;
  agent: string | null;
}): HomeStart {
  if (choice === ProjectChoice.Temp) {
    return { kind: "temp" };
  }

  const prompt = parseInput({
    schema: FirstPrompt,
    raw: rawPrompt
  }) ?? "";
  if (choice === ProjectChoice.New) {
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

/** The throwaway-workspace choice's name - the menu item and the trigger. */
export const THROWAWAY_LABEL = "Throwaway workspace";
const NEW_PROJECT_LABEL = "New project";
const NO_PROJECT_LABEL = "Choose a project";

/** The project menu trigger's label for what is picked: the throwaway
 *  workspace, the new project's typed name, or the chosen project's name. */
export function projectTriggerLabel({ choice, typedName, selectedName }: {
  choice: ProjectChoice;
  typedName: string;
  selectedName: string | null;
}) {
  if (choice === ProjectChoice.Temp) {
    return THROWAWAY_LABEL;
  }

  if (choice === ProjectChoice.New) {
    return typedName ? `New: ${typedName}` : NEW_PROJECT_LABEL;
  }

  return selectedName ?? NO_PROJECT_LABEL;
}
