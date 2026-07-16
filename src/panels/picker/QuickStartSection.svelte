<script lang="ts">
  import { workspace } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { FirstPrompt, nameError, parseInput, ProjectName } from "@/lib/validate";

  // "Start something new": the temp-workspace card and the create-a-project
  // form (root select + name + optional first prompt). Creation goes straight
  // through the bridge (it never touches settings); the chosen project path —
  // and the optional first prompt — go back to the app through `onopen`.
  const { roots, onopen }: {
    roots: string[];
    onopen: (target: {
      path: string;
      initialPrompt?: string;
    }) => void;
  } = $props();

  let createIn = $state("");
  let createName = $state("");
  let createPrompt = $state("");

  // Live name validation: surface the schema's message and gate the submit on
  // the same check, so an invalid name can't reach a create() that would
  // silently no-op.
  const createNameError = $derived(nameError(createName));
  const createNameValid = $derived(ProjectName.safeParse(createName).success);
</script>

<section class="new">
  <h2>Start something new</h2>
  <div class="new-grid">
    <!-- Start immediately in a throwaway workspace. -->
    <button
      class="temp-start"
      onclick={async () => {
        const path = await workspace.temp();
        onopen({ path });
      }}
    >
      <span class="ico"><Icon name="star" size={20} /></span>
      <span class="txt">
        <strong>Start in a temp workspace</strong>
        <small>A clean scratch folder — auto-named once the agent starts working.</small>
      </span>
    </button>

    <form
      class="np"
      aria-labelledby="np-title"
      onsubmit={async e => {
        e.preventDefault();
        const name = parseInput({
          schema: ProjectName,
          raw: createName
        });
        const prompt = parseInput({
          schema: FirstPrompt,
          raw: createPrompt
        });
        const promptInvalid = prompt === null;
        if (!createIn || !name || promptInvalid) {
          return;
        }

        const path = await workspace.create({
          root: createIn,
          name
        });
        onopen({
          path,
          initialPrompt: prompt || undefined
        });
      }}
    >
      <h3 id="np-title">Create a new project</h3>

      <div class="np-field">
        <span id="np-loc-label" class="np-label">Location</span>
        <div class="np-loc" aria-labelledby="np-loc-label" role="group">
          <span class="np-loc-ico" aria-hidden="true"><Icon name="folder" /></span>
          <span class="root-sel">
            <button
              style:anchor-name="--np-root"
              class="root-trigger"
              aria-label="Root folder"
              popovertarget="np-root-menu"
              type="button"
            >
              <span class="root-current">{createIn || "Choose a root…"}</span>
              <span class="caret" aria-hidden="true">▾</span>
            </button>
            <ul id="np-root-menu" style:position-anchor="--np-root" class="menu root-menu popover-menu" popover>
              {#each roots as root (root)}
                {@const isPicked = createIn === root}
                <li>
                  <button
                    class="mi root-opt"
                    class:picked={isPicked}
                    aria-current={isPicked}
                    onclick={() => (createIn = root)}
                    popovertarget="np-root-menu"
                    popovertargetaction="hide"
                    type="button"
                  >
                    <span>{root}</span>
                    {#if isPicked}
                      <span class="tick" aria-hidden="true">✓</span>
                    {/if}
                  </button>
                </li>
              {:else}
                <li class="none root-empty">No roots yet — add one below.</li>
              {/each}
            </ul>
          </span>
          <span class="np-sep" aria-hidden="true">\</span>
          <label class="visually-hidden" for="np-name">Project name</label>
          <input
            id="np-name"
            class="np-name"
            aria-describedby={createNameError ? "np-name-error" : undefined}
            aria-invalid={createNameError !== null}
            autocomplete="off"
            placeholder="project-name"
            spellcheck="false"
            bind:value={createName}
          />
        </div>
        {#if createNameError}
          <output id="np-name-error" class="field-error">{createNameError}</output>
        {/if}
      </div>

      <div class="np-field">
        <label class="np-label" for="np-prompt">
          First prompt <span class="np-optional">— optional</span>
        </label>
        <textarea
          id="np-prompt"
          class="np-prompt"
          placeholder="e.g. scaffold a SvelteKit app with Tailwind"
          rows="2"
          bind:value={createPrompt}
        ></textarea>
      </div>

      <button class="np-go" disabled={!createIn || !createNameValid} type="submit">
        Create &amp; open
      </button>
    </form>
  </div>
</section>

<style>
  /* ── Start something new — responsive 2-up: temp card + create form. ── */
  .new-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(268px, 1fr));
    gap: 12px;
    align-items: stretch;
  }

  /* Big scratch-workspace card — filled primary-container with a hairline
     primary edge; brightens on hover. */
  .temp-start {
    display: flex;
    flex-direction: column;
    gap: 8px;
    justify-content: center;
    padding: 20px 22px;
    border: 1px solid var(--primary);
    border-radius: var(--radius-large);
    background: var(--primary-container);
    color: var(--on-primary-container);
    text-align: start;
    cursor: pointer;
    transition: filter 150ms var(--ease);

    &:hover {
      filter: brightness(1.08);
    }

    .txt {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }

    strong {
      font-weight: 700;
      font-size: 16px;
    }

    small {
      color: var(--on-primary-container);
      font-size: 13px;
      opacity: 85%;
    }
  }

  /* Create-a-new-project form card — surface-1 with a hairline outline. */
  .np {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px 20px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-large);
    background: var(--surface-1);

    h3 {
      margin: 0;
      font-weight: 700;
      font-size: 16px;
    }
  }

  .np-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .np-label {
    color: var(--on-surface-variant);
    font-weight: 600;
    font-size: 11px;
    letter-spacing: 0.03em;
  }

  .np-optional {
    font-weight: 400;
    opacity: 75%;
  }

  /* The "Location" group row — folder icon, root select, "\" separator, name. */
  .np-loc {
    display: flex;
    gap: 2px;
    align-items: center;
    padding-block: 4px;
    padding-inline: 12px 4px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);

    .np-loc-ico {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }

    .np-sep {
      flex: none;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 13px;
    }
  }

  .np-name {
    flex: 1 1 90px;
    min-inline-size: 80px;
    padding: 6px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 13px;
  }

  .np-prompt {
    inline-size: 100%;
    padding: 9px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-size: 13px;
    line-height: 1.5;
    resize: vertical;
  }

  .np-go {
    align-self: start;
    padding: 10px 20px;
    border: none;
    border-radius: var(--radius-medium);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 700;
    font-size: 13px;
    cursor: pointer;
    transition: filter 150ms var(--ease);

    &:hover:not(:disabled) {
      filter: brightness(1.06);
    }

    &:disabled {
      opacity: 50%;
      cursor: default;
    }
  }

  /* Root select — a native-popover custom select, like the editor selects. */
  .root-sel {
    position: relative;
    flex: 1 1 auto;
    min-inline-size: 0;
  }

  .root-trigger {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    max-inline-size: 150px;
    padding: 6px 2px;
    border: none;
    background: transparent;
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: color 150ms var(--ease);

    &:hover {
      color: var(--primary);
    }

    .root-current {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .caret {
      display: inline-block;
      flex: none;
      font-size: 9px;
      opacity: 70%;
      transition: rotate 150ms var(--ease);
    }
  }

  /* Open state — while the menu is open the trigger reads active and the caret
     flips (inspired by the youtube-downloader select). Pure CSS, field-scoped. */
  .root-sel:has(.root-menu:popover-open) {
    .root-trigger {
      color: var(--primary);
    }

    .caret {
      rotate: 180deg;
    }
  }

  .root-menu {
    min-inline-size: 240px;

    .root-opt {
      justify-content: space-between;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;
    }

    .root-empty {
      padding: 8px 10px;
    }
  }
</style>
