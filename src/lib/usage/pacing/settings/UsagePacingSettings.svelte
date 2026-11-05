<script lang="ts">
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { prefs } from "@/lib/settings/settings.svelte";
  import type { AccountUsage, PacedWindowKind, PacingSettings } from "@/lib/types";
  import Segmented from "@/lib/ui/controls/Segmented.svelte";
  import {
    globalPacing,
    projectWindowPacing,
    resolvedPacing,
    withGlobalWindowPacing,
    withProjectWindowPacing
  } from "@/lib/usage/pacing/pacing-scope";
  import PacingWindowSettings from "@/lib/usage/pacing/settings/PacingWindowSettings.svelte";
  import { reportedPacedKinds } from "@/lib/usage/pacing/usage-pace";
  import { settledValue } from "@/lib/util/async/settled";
  import { untrack } from "svelte";

  // Usage pacing's settings, one block per paced window (session, weekly), and
  // which scope they edit (lib/usage/pacing/pacing-scope): the global settings every
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

  // Only the windows the agent's plan has: Claude Ultra reports a weekly limit
  // and no session one, so it gets no session settings. The backend caches the
  // reading for a few minutes.
  let account = $state<AccountUsage | null>(null);
  $effect(() => {
    const agentId = agent;
    untrack(() => loadAccount(agentId));
  });

  async function loadAccount(agentId: string) {
    const reading = await settledValue({
      promise: sendMessage(Command.usageAccountAgent, {
        agent: agentId,
        forceRefresh: false
      }),
      fallback: null
    });
    if (agentId === agent) {
      account = reading?.account ?? null;
    }
  }

  const shownKinds = $derived(reportedPacedKinds(account));

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

<section class="pacing" aria-label="Usage pacing">
  <header class="pacing-head">
    <span class="eyebrow section-label">Usage pacing</span>
    <Segmented
      chosen={scope}
      label="Which usage pacing settings to edit"
      onchoose={value => {
        scope = value;
      }}
      options={scopeOptions}
    />
  </header>
  <span class="field-hint scope-hint">{SCOPE_HINTS[scope]}</span>

  {#each shownKinds as kind (kind)}
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
</section>

<style>
  /* A section of the usage panel, framed like its spotlight card. */
  .pacing {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-block-end: 14px;
    padding-block: 12px;
    padding-inline: 13px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
  }

  .pacing-head {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
  }

  .scope-hint {
    margin-block-start: -6px;
  }

  p {
    margin: 0;
  }

  /* The field vocabulary the pacing parts write in (`.field`, `.field-row`,
     `.field-text`, `.field-label`, `.field-hint`), at the usage panel's smaller
     scale, at zero specificity (`:where`) so a part's own rule still wins. */
  :where(.pacing) :global(.field) {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  :where(.pacing) :global(.field-row) {
    display: flex;
    gap: 10px;
    justify-content: space-between;
    align-items: center;
  }

  :where(.pacing) :global(.field-text) {
    display: flex;
    flex-direction: column;
    gap: 1px;
    line-height: 1.3;
  }

  :where(.pacing) :global(.field-label) {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 0.6875rem;
  }

  :where(.pacing) :global(.field-hint) {
    color: var(--on-surface-variant);
    font-size: 0.6563rem;
  }

  /* The design's pacing-mode row is a notch tighter than a Config switch. */
  :where(.pacing) :global(.segmented .option) {
    padding-block: 6px;
    padding-inline: 6px;
    font-size: 0.6875rem;
    white-space: nowrap;
  }
</style>
