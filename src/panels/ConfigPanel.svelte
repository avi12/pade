<script lang="ts">
  import { setPanelHeader } from "@/lib/stores/ui/sidePanel.svelte";
  import type { Agent } from "@/lib/types";
  import AppearanceSettings from "@/panels/config/appearance/AppearanceSettings.svelte";
  import DiscordSettings from "@/panels/config/DiscordSettings.svelte";
  import ProjectFiles from "@/panels/config/files/ProjectFiles.svelte";
  import AgentModelDefaults from "@/panels/config/models/AgentModelDefaults.svelte";
  import UsagePacingSettings from "@/panels/config/pacing/UsagePacingSettings.svelte";
  import PerformanceSettings from "@/panels/config/PerformanceSettings.svelte";
  import SessionTabsSettings from "@/panels/config/SessionTabsSettings.svelte";
  import SettingsCard from "@/panels/config/SettingsCard.svelte";
  import UpdateSettings from "@/panels/config/UpdateSettings.svelte";

  // Only the config files relevant to the active agent are listed. `project` is
  // what scopes the per-project settings below to this window's workspace.
  const { agent, agents, project }: {
    agent: string;
    /** The installed agents - the only ones a starting model is offered for. */
    agents: Agent[];
    project: string;
  } = $props();

  // Config has no count or refresh - clear those slots in the shared header.
  $effect(() => {
    setPanelHeader({
      count: null,
      refresh: null
    });
  });
</script>

<div class="configuration">
  <div class="scroll">
    <SettingsCard title="Appearance">
      <AppearanceSettings />
    </SettingsCard>

    <SettingsCard title="Usage pacing">
      <UsagePacingSettings {agent} {project} />
    </SettingsCard>

    <SettingsCard isCompact title="Session tabs">
      <SessionTabsSettings {project} />
    </SettingsCard>

    <SettingsCard title="Starting model">
      <p class="field-hint">
        New sessions in this workspace start on these - each tab can still switch from its model tag
      </p>
      <AgentModelDefaults {agents} {project} />
    </SettingsCard>

    <SettingsCard title="Performance">
      <PerformanceSettings />
    </SettingsCard>

    <SettingsCard isCompact title="Discord">
      <DiscordSettings />
    </SettingsCard>

    <SettingsCard isCompact title="Updates">
      <UpdateSettings />
    </SettingsCard>

    <ProjectFiles {agent} />
  </div>
</div>

<style>
  .configuration {
    display: flex;
    flex-direction: column;
    block-size: 100%;
  }

  .scroll {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    min-block-size: 0;
    padding: 10px;
    animation: panel-swap 280ms var(--ease);
  }
</style>
