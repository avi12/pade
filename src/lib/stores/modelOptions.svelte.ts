// Each agent's switchable models, from the agent's own local catalog (see
// running_model.rs): names, effort levels, the recommended level. Loaded once
// per agent and refreshed whenever a model menu opens, so a catalog the agent
// re-fetched in the meantime shows up.

import { agents } from "@/lib/bridge";
import type { ModelOption } from "@/lib/bridge";
import { bareModel } from "@/lib/model-label";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

const optionsByAgent = new SvelteMap<string, ModelOption[]>();
const requestedCommands = new SvelteSet<string>();

/** Rejects when the backend call fails; the previous options stay in place. */
async function loadModelOptions(command: string) {
  optionsByAgent.set(command, await agents.modelOptions(command));
}

/** Load `command`'s catalog the first time anything asks for it - the tab's
 *  bars need the running model's own ladder without waiting for a menu. */
export async function ensureModelOptions(command: string) {
  if (requestedCommands.has(command)) {
    return;
  }

  requestedCommands.add(command);
  try {
    await loadModelOptions(command);
  } catch (error) {
    requestedCommands.delete(command);
    console.warn(`pade: could not load ${command}'s model catalog`, error);
  }
}

/** Re-read `command`'s catalog - a model menu opening, which should show what
 *  the agent fetched since. A failure keeps the options already loaded. */
export async function refreshModelOptions(command: string) {
  requestedCommands.add(command);
  try {
    await loadModelOptions(command);
  } catch (error) {
    console.warn(`pade: could not refresh ${command}'s model catalog`, error);
  }
}

/** The models `command` offers, empty until loaded. */
export function modelOptions(command: string) {
  return optionsByAgent.get(command) ?? [];
}

/** The catalog entry for a running model. A reported id may carry a
 *  `provider/` prefix the catalog doesn't, or the reverse, so both forms match. */
export function modelOption({ command, model }: {
  command: string;
  model: string;
}) {
  const bare = bareModel(model);
  return modelOptions(command).find(option => option.id === model || bareModel(option.id) === bare);
}
