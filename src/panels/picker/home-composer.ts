import type { OpenTarget } from "@/lib/types";
import { baseName, projectRootName } from "@/lib/util/paths";
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

/** Why a new project's typed name can't be created, or null when it can:
 *  the name schema's own reason first, then whatever already stands on disk
 *  under that name in the root. */
export function newProjectNameProblem({ nameError, isTakenByFile, isExistingFolder }: {
  nameError: string | null;
  isTakenByFile: boolean;
  isExistingFolder: boolean;
}) {
  if (nameError) {
    return nameError;
  }

  if (isTakenByFile) {
    return "That name is taken by a file in this root";
  }

  if (isExistingFolder) {
    return "This folder already exists in this root";
  }

  return null;
}

/** The throwaway-workspace choice's name - the menu item and the trigger. */
export const THROWAWAY_LABEL = "Throwaway workspace";
/** The name a new project shows until one is typed - the name field's
 *  placeholder and the trigger's muted stand-in. */
export const NEW_PROJECT_PLACEHOLDER = "new-project";
const NO_PROJECT_LABEL = "Choose a project";

/** What the project menu trigger reads for what is picked, in the menu rows'
 *  "root/name" format: the throwaway workspace; a new project as the root it
 *  goes under and its typed name - the placeholder name, muted, until one is
 *  typed - so the trigger says where Start will create it; or the chosen
 *  project with its root. */
export function projectTrigger({ choice, typedName, root, selectedName, selectedPath }: {
  choice: ProjectChoice;
  typedName: string;
  /** The root a new project goes under (empty while none is chosen). */
  root: string;
  selectedName: string | null;
  selectedPath: string;
}) {
  if (choice === ProjectChoice.Temp) {
    return {
      root: "",
      name: THROWAWAY_LABEL,
      isPlaceholder: false
    };
  }

  if (choice === ProjectChoice.New) {
    return {
      root: root ? baseName(root) : "",
      name: typedName || NEW_PROJECT_PLACEHOLDER,
      isPlaceholder: !typedName
    };
  }

  if (selectedName === null) {
    return {
      root: "",
      name: NO_PROJECT_LABEL,
      isPlaceholder: false
    };
  }

  return {
    root: projectRootName(selectedPath),
    name: selectedName,
    isPlaceholder: false
  };
}
