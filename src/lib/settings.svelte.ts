import { createVersionedAdopter, replaceRecord } from "@/lib/settings-state";
import type { Prefs, Settings } from "@/lib/types";

export const prefs = $state<Prefs>({});
export const settings = $state<Settings>({
  roots: [],
  defaultAgent: null,
  projectAgents: {},
  recentProjects: [],
  pinnedProjects: [],
  ownedWorkspaces: [],
  labels: {},
  workspaceLastUsed: {},
  prefs
});

function noAdoptionEffect() {}
let afterAdopt: () => void = noAdoptionEffect;

function replaceSettings(fresh: Settings) {
  replaceRecord({
    target: prefs,
    fresh: fresh.prefs
  });
  replaceRecord({
    target: settings,
    fresh: {
      ...fresh,
      prefs
    }
  });
  afterAdopt();
}

const settingsVersions = createVersionedAdopter<Settings>({ adopt: replaceSettings });

export function registerSettingsAdoptionEffect(effect: () => void) {
  afterAdopt = effect;
}

export function beginSettingsRequest() {
  return settingsVersions.begin();
}

export function adoptSettings({ fresh, version }: {
  fresh: Settings;
  version: number;
}) {
  return settingsVersions.adopt({
    fresh,
    version
  });
}
