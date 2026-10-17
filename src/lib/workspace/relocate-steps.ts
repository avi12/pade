import { RelocateStep } from "@/lib/types";

/** What a prompt waiting on a folder move says at each of its steps. */
export const RELOCATE_STEP_TEXT = {
  [RelocateStep.enum.renaming]: "Renaming the folder…",
  [RelocateStep.enum.agentMemories]: "Moving the agents’ memory of it…",
  [RelocateStep.enum.editorRecents]: "Updating your editors’ recent lists…",
  [RelocateStep.enum.links]: "Re-pointing the links inside it…",
  [RelocateStep.enum.settings]: "Saving PADE’s settings…"
} satisfies Record<RelocateStep, string>;
