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
  prefs
});

function noAdoptionEffect(): void {}
let afterAdopt: () => void = noAdoptionEffect;

function replaceSettings(fresh: Settings): void {
  replaceRecord(prefs, fresh.prefs);
  replaceRecord(settings, {
    ...fresh,
    prefs
  });
  afterAdopt();
}

const settingsVersions = createVersionedAdopter<Settings>({ adopt: replaceSettings });

export function registerSettingsAdoptionEffect(effect: () => void): void {
  afterAdopt = effect;
}

export function beginSettingsRequest(): number {
  return settingsVersions.begin();
}

export function adoptSettings(fresh: Settings, version: number): boolean {
  return settingsVersions.adopt(fresh, version);
}
