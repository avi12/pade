<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import type { Agent, OpenTarget } from "@/lib/types";
  import { rovingTablist } from "@/lib/ui/roving-tabs";
  import ClonePanel from "@/panels/picker/ClonePanel.svelte";
  import CreateProjectPanel from "@/panels/picker/CreateProjectPanel.svelte";
  import OpenLocalPanel from "@/panels/picker/OpenLocalPanel.svelte";
  import "@/panels/picker/quick-start.css";
  import { onMount, tick } from "svelte";

  // "Get started": one card, three ways in - New (create a project, or a
  // throwaway temp workspace when the name stays blank), Local (open an
  // existing folder), Clone (git clone a repository, gated on git being
  // installed). Each way in is its own panel component; this card owns the
  // tabs, the height glide between panels and where the caret lands. The
  // chosen project path - and, for New, the optional first prompt - go back to
  // the app through `onopen`. `createIn` is bindable so the picker can fill it
  // when a root is selected elsewhere; `onnewroot` jumps to the Root folders
  // add field for a root not listed yet.
  let { roots, agents, defaultAgent, onopen, onnewroot, createIn = $bindable("") }: {
    roots: string[];
    agents: Agent[];
    defaultAgent: string | null;
    onopen: (target: OpenTarget) => void;
    onnewroot: () => void;
    createIn?: string;
  } = $props();

  // The three ways in - a closed set, compared against by member.
  const StartTab = {
    Create: "create",
    Local: "local",
    Clone: "clone"
  } as const;
  type StartTab = (typeof StartTab)[keyof typeof StartTab];
  const TABS: readonly {
    id: StartTab;
    label: string;
    icon: IconName;
  }[] = [
    {
      id: StartTab.Create,
      label: "New project",
      icon: "sparkles"
    },
    {
      id: StartTab.Local,
      label: "Open local",
      icon: "folder"
    },
    {
      id: StartTab.Clone,
      label: "Clone repo",
      icon: "git"
    }
  ];
  let tab = $state<StartTab>(StartTab.Create);
  let elCard = $state<HTMLDivElement | null>(null);
  // The panels container and the height it should occupy - the open panel's
  // own measured height, kept current by a ResizeObserver so the container
  // glides (CSS transition) on tab switches AND when the open panel itself
  // grows or shrinks (a credentials panel appearing, an error line).
  let elPanels = $state<HTMLDivElement | null>(null);
  let panelsHeight = $state<number | null>(null);

  $effect(() => {
    const body = elPanels;
    // Re-arm on every tab switch - the observer must follow the newly-open slot,
    // selected by the active tab so this effect re-runs whenever it changes.
    const openSlot = body?.querySelector(`.slot[data-tab="${tab}"]`);
    if (!(openSlot instanceof HTMLElement)) {
      return;
    }

    const observer = new ResizeObserver(() => {
      panelsHeight = openSlot.getBoundingClientRect().height;
    });
    observer.observe(openSlot);
    return () => observer.disconnect();
  });

  // The newly-open slot flips `visibility: hidden → visible` this tick, but that
  // change only takes hold once the browser has laid it out - focusing a
  // still-hidden element is a silent no-op. Waiting two frames lets the
  // visibility actually land before we place the caret.
  function afterVisibilityApplies() {
    return new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  }

  // Land the caret on the first field still waiting for input - the name if it's
  // blank, otherwise the next empty box (the first prompt) - so opening the New
  // tab with a name already typed drops you straight into the prompt instead of
  // re-selecting a filled field. `:placeholder-shown` is the platform's own
  // "this field is empty" state (every field here carries a placeholder), so the
  // first field still showing it is the first one awaiting input; fall back to the
  // first field when all are filled.
  function focusFirstEmptyField() {
    const panel = elCard?.querySelector<HTMLElement>(".open .panel");
    if (!panel) {
      return;
    }

    const firstEmpty = panel.querySelector<HTMLElement>(
      "input:placeholder-shown, textarea:placeholder-shown"
    );
    (firstEmpty ?? panel.querySelector<HTMLElement>("input, textarea"))?.focus();
  }

  // The New tab is open on arrival - land the caret in its first empty field
  // (the same behaviour as clicking the tab), so opening the picker to start a
  // project drops you straight into the name/prompt.
  onMount(async () => {
    await afterVisibilityApplies();
    focusFirstEmptyField();
  });
</script>

<section class="get-started">
  <h2>Get started</h2>
  <div bind:this={elCard} class="card">
    <div class="pill-tabs" aria-label="How to open a project" role="tablist" use:rovingTablist>
      {#each TABS as { id, label, icon } (id)}
        <button
          id={`gs-tab-${id}`}
          class="pill-tab"
          aria-selected={tab === id}
          onclick={async e => {
            tab = id;
            // A pointer click (detail > 0) lands the user in the panel's first
            // input; arrow-key selection (a programmatic click, detail 0) keeps
            // focus on the tab so arrowing onward still works - Tab enters.
            const isPointerClick = e.detail > 0;
            if (!isPointerClick) {
              return;
            }

            await tick();
            await afterVisibilityApplies();
            focusFirstEmptyField();
          }}
          role="tab"
          tabindex={tab === id ? 0 : -1}
          type="button"
        >
          <Icon name={icon} size={14} />
          {label}
        </button>
      {/each}
    </div>

    <!-- Every panel stays mounted, overlaid in ONE grid cell, so a switch
         swaps content in place (a cross-fade, nothing travels) while the
         container's height glides to the open panel's measured size. Panels
         keep their typed state across switches. -->
    <div
      bind:this={elPanels}
      style:block-size={panelsHeight === null ? "auto" : `${panelsHeight}px`}
      class="panels"
    >
      <div class="slot" class:open={tab === StartTab.Create} data-tab={StartTab.Create}>
        <CreateProjectPanel {agents} {defaultAgent} {onnewroot} {onopen} {roots} bind:createIn />
      </div>
      <div class="slot" class:open={tab === StartTab.Local} data-tab={StartTab.Local}>
        <OpenLocalPanel isActive={tab === StartTab.Local} {onopen} />
      </div>
      <div class="slot" class:open={tab === StartTab.Clone} data-tab={StartTab.Clone}>
        <ClonePanel {onnewroot} {onopen} {roots} bind:createIn />
      </div>
    </div>
  </div>
</section>

<style>
  /* ── Get started - one card, three tabbed ways in. ── */
  .card {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 18px 20px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-large);
    background: var(--surface-1);

    /* The mode tabs read a notch larger than the on-launch pair. */
    .pill-tab {
      padding-block: 7px;
      font-size: 13px;
    }
  }

  /* All panels overlay one grid cell: a tab switch is a pure in-place
     cross-fade - nothing slides - while the container's block-size (the open
     panel's measured height, CSS-transitioned) glides between panel sizes.
     `visibility` rides the fade so a closed panel is untabbable/invisible
     exactly when it has faded out; the clip margin keeps focus rings visible
     while the container sits exactly at the open panel's height. */
  .panels {
    display: grid;
    overflow: clip;
    overflow-clip-margin: 6px;
    transition: block-size 300ms var(--ease);
  }

  .slot {
    grid-area: 1 / 1;
    align-self: start;
    opacity: 0%;
    visibility: hidden;
    transition:
      opacity 240ms var(--ease),
      visibility 240ms;

    &.open {
      opacity: 100%;
      visibility: visible;
    }
  }

  /* The panels' shared field chrome lives in picker/quick-start.css; each
     panel's own look lives in its component (CreateProjectPanel,
     OpenLocalPanel, ClonePanel, RootSelect). */
</style>
