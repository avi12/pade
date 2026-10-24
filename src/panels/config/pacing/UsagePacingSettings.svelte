<script lang="ts">
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { prefs } from "@/lib/settings/settings.svelte";
  import { PacedWindowKind } from "@/lib/types";
  import type { PacingSettings } from "@/lib/types";
  import Segmented from "@/lib/ui/Segmented.svelte";
  import {
    globalPacing,
    projectWindowPacing,
    resolvedPacing,
    withGlobalWindowPacing,
    withProjectWindowPacing
  } from "@/lib/usage/pacing-scope";
  import PacingWindowSettings from "@/panels/config/pacing/PacingWindowSettings.svelte";

  // Usage pacing's settings, one block per paced window (session, weekly), and
  // which scope they edit (lib/usage/pacing-scope): the global settings every
  // workspace follows, or this workspace's own. A window changed in the
  // workspace scope stops following the global one until "Use global".
  const { agent, project }: {
    /** The active agent, whose account the live lines read. */
    agent: string;
    project: string;
  } = $props();

  const PacingScope = {
    Workspace: "workspace",
    Global: "global"
  } as const;
  type PacingScope = (typeof PacingScope)[keyof typeof PacingScope];

  const scopeOptions = [
    {
      value: PacingScope.Workspace,
      label: "This workspace"
    },
    {
      value: PacingScope.Global,
      label: "Global"
    }
  ] as const;
  const SCOPE_HINTS: Record<PacingScope, string> = {
    [PacingScope.Workspace]: "Changes here apply to this workspace only",
    [PacingScope.Global]: "Every workspace without its own settings follows these"
  };

  let scope = $state<PacingScope>(PacingScope.Workspace);
  const isWorkspaceScope = $derived(scope === PacingScope.Workspace);
  const shown = $derived.by(() => {
    if (isWorkspaceScope) {
      return resolvedPacing({
        prefs,
        project
      });
    }

    return globalPacing(prefs);
  });

  /** In the workspace scope, whether a window follows the global settings and
   *  how it goes back to them; nothing in the global scope. */
  function inheritFor(kind: PacedWindowKind) {
    if (!isWorkspaceScope) {
      return undefined;
    }

    return {
      isFollowing: projectWindowPacing({
        prefs,
        project,
        kind
      }) === undefined,
      onreset: async () => await save({
        kind,
        settings: undefined
      })
    };
  }

  async function save({ kind, settings }: {
    kind: PacedWindowKind;
    settings: PacingSettings | undefined;
  }) {
    if (isWorkspaceScope) {
      await updatePrefs({
        pacingProjects: withProjectWindowPacing({
          prefs,
          project,
          kind,
          settings
        })
      });
      return;
    }

    if (settings === undefined) {
      return;
    }

    await updatePrefs({
      pacing: withGlobalWindowPacing({
        prefs,
        kind,
        settings
      })
    });
  }
</script>

<div class="field">
  <Segmented
    chosen={scope}
    label="Which usage pacing settings to edit"
    onchoose={value => {
      scope = value;
    }}
    options={scopeOptions}
  />
  <span class="field-hint">{SCOPE_HINTS[scope]}</span>
</div>

{#each PacedWindowKind.options as kind (kind)}
  <PacingWindowSettings
    {agent}
    inherit={inheritFor(kind)}
    {kind}
    onsave={async settings => await save({
      kind,
      settings
    })}
    settings={shown[kind]}
  />
{/each}

<p class="field-hint">Every step undoes itself as room comes back, and everything reverts when the window resets</p>

<style>
  p {
    margin: 0;
  }
</style>
