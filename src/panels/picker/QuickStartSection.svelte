<script lang="ts">
  import { os, vcs, workspace } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import { rovingTablist } from "@/lib/rovingTabs";
  import {
    CloneUrl,
    FirstPrompt,
    FolderPath,
    GitSecret,
    GitUsername,
    isSshCloneUrl,
    nameError,
    parseInput,
    ProjectName,
    repoFolderName
  } from "@/lib/validate";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onMount, tick } from "svelte";

  // "Get started": one card, three ways in — New (create a project, or a
  // throwaway temp workspace when the name stays blank), Local (open an
  // existing folder), Clone (git clone a repository, gated on git being
  // installed). Creation goes straight through the bridge; the chosen project
  // path — and, for New, the optional first prompt — go back to the app
  // through `onopen`. `createIn` is bindable so the picker can fill it when a
  // root is selected elsewhere; `onnewroot` jumps to the Root folders add
  // field for a root not listed yet.
  let { roots, onopen, onnewroot, createIn = $bindable("") }: {
    roots: string[];
    onopen: (target: {
      path: string;
      initialPrompt?: string;
    }) => void;
    onnewroot: () => void;
    createIn?: string;
  } = $props();

  // The three ways in — a closed set, compared against by member.
  const StartTab = {
    create: "create",
    local: "local",
    clone: "clone"
  } as const;
  type StartTab = (typeof StartTab)[keyof typeof StartTab];
  const TABS: readonly {
    id: StartTab;
    label: string;
    icon: IconName;
  }[] = [
    {
      id: StartTab.create,
      label: "New",
      icon: "sparkles"
    },
    {
      id: StartTab.local,
      label: "Local",
      icon: "folder"
    },
    {
      id: StartTab.clone,
      label: "Clone",
      icon: "git"
    }
  ];
  let tab = $state<StartTab>(StartTab.create);
  let card = $state<HTMLDivElement | null>(null);

  // The root menu element — "New root folder…" hides it imperatively BEFORE
  // handing off, because the declarative popovertargetaction="hide" runs after
  // the click handler and would cancel the smooth scroll to the add field.
  let rootMenu = $state<HTMLUListElement | null>(null);

  // ── New — create a project (or fall through to a temp workspace) ──────────
  let createName = $state("");
  let createPrompt = $state("");

  // Live name validation: surface the schema's message and gate the submit on
  // the same check, so an invalid name can't reach a create() that would
  // silently no-op. An empty name is fine — it means "temp workspace".
  const createNameError = $derived(nameError(createName));
  const hasTypedName = $derived(createName.trim().length > 0);
  const createDisabled = $derived(
    hasTypedName && (!createIn || createNameError !== null)
  );

  // Shared by the "…or start a throwaway temp workspace" button and a submit
  // with the name left blank — the two spellings of the same intent.
  async function startTemp(prompt: string) {
    const path = await workspace.temp();
    onopen({
      path,
      initialPrompt: prompt || undefined
    });
  }

  // ── Local — open an existing folder ───────────────────────────────────────
  let localPath = $state("");
  // The latest probe, tagged with the path it described — only a settled probe
  // (disk knowledge about the *current* text) gates the button or complains.
  let localProbe = $state<{
    path: string;
    isDir: boolean;
  }>({
    path: "",
    isDir: false
  });
  const trimmedLocal = $derived(localPath.trim());
  const localSettled = $derived(trimmedLocal.length > 0 && localProbe.path === trimmedLocal);
  const localIsDir = $derived(localSettled && localProbe.isDir);
  const localError = $derived(
    localSettled && !localProbe.isDir ? "That folder doesn’t exist." : null
  );

  $effect(() => {
    const path = trimmedLocal;
    if (path.length === 0) {
      localProbe = {
        path: "",
        isDir: false
      };
      return;
    }

    const timer = setTimeout(async () => {
      const result = await workspace.probePath(path);
      localProbe = {
        path,
        isDir: result.isDir
      };
    }, 120);
    return () => clearTimeout(timer);
  });

  // ── Clone — git clone a repository ─────────────────────────────────────────
  // `null` until the probe answers, so the panel is born knowing whether git
  // exists instead of flashing the wrong body.
  let gitInstalled = $state<boolean | null>(null);
  let hasSshKey = $state(false);
  let cloneUrl = $state("");
  let cloneName = $state("");
  // Auto-fill the folder name from the URL only until the user edits it.
  let cloneNameEdited = $state(false);
  let cloneUsername = $state("");
  let clonePassword = $state("");
  let cloning = $state(false);
  let cloneError = $state("");

  const cloneUrlValid = $derived(CloneUrl.safeParse(cloneUrl).success);
  const cloneNameValid = $derived(ProjectName.safeParse(cloneName).success);
  // An SSH-style URL with no key on disk can't authenticate — offer HTTPS
  // credentials instead (the backend rewrites the URL for the clone).
  const needsCredentials = $derived(cloneUrlValid && isSshCloneUrl(cloneUrl) && !hasSshKey);
  const credentialsMissing = $derived(
    needsCredentials
    && !(GitUsername.safeParse(cloneUsername).success && GitSecret.safeParse(clonePassword).success)
  );
  const cloneDisabled = $derived(
    !cloneUrlValid || !createIn || !cloneNameValid || credentialsMissing || cloning
  );

  $effect(() => {
    if (cloneNameEdited) {
      return;
    }

    cloneName = repoFolderName(cloneUrl);
  });

  // Shared by mount and the "Re-check" button after installing git.
  async function checkGit() {
    const [installed, keyOnDisk] = await Promise.all([vcs.gitInstalled(), vcs.hasSshKey()]);
    gitInstalled = installed;
    hasSshKey = keyOnDisk;
  }

  onMount(async () => {
    await checkGit();
  });
</script>

<!-- The root select — trigger + anchored menu incl. the jump-to-add action.
     Shared by the New and Clone location rows; only one is ever mounted, so
     the menu id and anchor name can stay fixed. -->
{#snippet rootSelect()}
  <span class="root-sel">
    <button
      class="root-trigger"
      aria-label="Root folder"
      popovertarget="np-root-menu"
      type="button"
    >
      <span class="root-current">{createIn || "Choose a root…"}</span>
      <span class="caret" aria-hidden="true">▾</span>
    </button>
    <ul
      bind:this={rootMenu}
      id="np-root-menu"
      style:position-anchor="--np-root"
      class="menu root-menu popover-menu"
      popover
    >
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
      {/each}
      <li class:separated={roots.length > 0}>
        <button
          class="mi root-new"
          onclick={() => {
            rootMenu?.hidePopover();
            onnewroot();
          }}
          type="button"
        >
          <Icon name="folderPlus" size={15} />
          <span>New root folder…</span>
        </button>
      </li>
    </ul>
  </span>
{/snippet}

<section class="get-started">
  <h2>Get started</h2>
  <div bind:this={card} class="card">
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
            // focus on the tab so arrowing onward still works — Tab enters.
            const isPointerClick = e.detail > 0;
            if (!isPointerClick) {
              return;
            }

            await tick();
            card?.querySelector<HTMLElement>(".panel input, .panel textarea")?.focus();
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

    {#if tab === StartTab.create}
      <form
        class="panel"
        onsubmit={async e => {
          e.preventDefault();
          const prompt = parseInput({
            schema: FirstPrompt,
            raw: createPrompt
          });
          if (prompt === null) {
            return;
          }

          if (!hasTypedName) {
            await startTemp(prompt);
            return;
          }

          const name = parseInput({
            schema: ProjectName,
            raw: createName
          });
          if (!createIn || !name) {
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
        <p class="hint">
          Name a folder to create a project — or leave the name blank for a
          throwaway temp workspace, auto-named once the agent starts working.
        </p>

        <div class="np-field">
          <span id="np-loc-label" class="np-label">Location</span>
          <div style:anchor-name="--np-root" class="np-loc" aria-labelledby="np-loc-label" role="group">
            <span class="np-loc-ico" aria-hidden="true"><Icon name="folder" /></span>
            {@render rootSelect()}
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

        <div class="actions">
          <button class="go" disabled={createDisabled} type="submit">
            Create &amp; open
          </button>
          <button
            class="temp-link"
            onclick={async () => {
              const prompt = parseInput({
                schema: FirstPrompt,
                raw: createPrompt
              });
              if (prompt === null) {
                return;
              }

              await startTemp(prompt);
            }}
            type="button"
          >
            …or start a throwaway temp workspace
          </button>
        </div>
      </form>
    {:else if tab === StartTab.local}
      <form
        class="panel"
        onsubmit={e => {
          e.preventDefault();
          const path = parseInput({
            schema: FolderPath,
            raw: localPath
          });
          if (!path || !localIsDir) {
            return;
          }

          onopen({ path });
        }}
      >
        <div class="np-field">
          <label class="np-label" for="local-path">Project folder</label>
          <div class="np-loc">
            <span class="np-loc-ico" aria-hidden="true"><Icon name="folder" /></span>
            <input
              id="local-path"
              class="path-input"
              autocomplete="off"
              placeholder="C:\repositories\my-app"
              spellcheck="false"
              bind:value={localPath}
            />
            <button
              class="browse"
              onclick={async () => {
                const picked = await openDialog({
                  directory: true,
                  multiple: false
                });
                if (typeof picked === "string") {
                  localPath = picked;
                }
              }}
              type="button"
            >Browse…</button>
          </div>
          {#if localError}
            <output class="field-error">{localError}</output>
          {/if}
        </div>

        <button class="go" disabled={!localIsDir} type="submit">Open project</button>
      </form>
    {:else if gitInstalled !== null}
      {#if !gitInstalled}
        <div class="panel warn-card">
          <p class="warn-head">
            <span class="warn-ico" aria-hidden="true"><Icon name="alert" size={16} /></span>
            <strong>Git isn’t installed</strong>
          </p>
          <p class="hint">PADE needs Git to clone a repository. Install it, then press Re-check.</p>
          <div class="actions">
            <button class="go" onclick={() => os.openUrl("https://git-scm.com/downloads")} type="button">
              Install Git…
            </button>
            <button class="ghost" onclick={async () => await checkGit()} type="button">Re-check</button>
          </div>
        </div>
      {:else}
        <form
          class="panel"
          onsubmit={async e => {
            e.preventDefault();
            const url = parseInput({
              schema: CloneUrl,
              raw: cloneUrl
            });
            const name = parseInput({
              schema: ProjectName,
              raw: cloneName
            });
            if (!url || !name || !createIn || cloneDisabled) {
              return;
            }

            cloning = true;
            cloneError = "";
            try {
              const path = await vcs.clone({
                url,
                root: createIn,
                name,
                username: needsCredentials ? (parseInput({
                  schema: GitUsername,
                  raw: cloneUsername
                }) ?? undefined) : undefined,
                password: needsCredentials ? (parseInput({
                  schema: GitSecret,
                  raw: clonePassword
                }) ?? undefined) : undefined
              });
              onopen({ path });
            } catch (error) {
              cloneError = typeof error === "string" ? error : "Clone failed.";
            } finally {
              cloning = false;
            }
          }}
        >
          <div class="np-field">
            <label class="np-label" for="clone-url">Repository URL</label>
            <input
              id="clone-url"
              class="path-input framed"
              autocomplete="off"
              placeholder="https://github.com/org/repo.git  ·  git@github.com:org/repo.git"
              spellcheck="false"
              bind:value={cloneUrl}
            />
          </div>

          <div class="np-field">
            <span id="clone-loc-label" class="np-label">
              Clone into <span class="np-optional">— folder name auto-filled from the repo</span>
            </span>
            <div style:anchor-name="--np-root" class="np-loc" aria-labelledby="clone-loc-label" role="group">
              <span class="np-loc-ico" aria-hidden="true"><Icon name="folder" /></span>
              {@render rootSelect()}
              <span class="np-sep" aria-hidden="true">\</span>
              <label class="visually-hidden" for="clone-name">Folder name</label>
              <input
                id="clone-name"
                class="np-name"
                autocomplete="off"
                oninput={() => (cloneNameEdited = true)}
                placeholder="repo"
                spellcheck="false"
                bind:value={cloneName}
              />
            </div>
          </div>

          {#if needsCredentials}
            <div class="warn-card creds">
              <p class="warn-head">
                <span class="warn-ico" aria-hidden="true"><Icon name="alert" size={16} /></span>
                <span>No SSH key set up for this host — sign in to clone over HTTPS instead.</span>
              </p>
              <div class="np-field">
                <label class="np-label" for="clone-user">Email or username</label>
                <input
                  id="clone-user"
                  class="cred-input"
                  autocomplete="username"
                  placeholder="you@example.com"
                  spellcheck="false"
                  bind:value={cloneUsername}
                />
              </div>
              <div class="np-field">
                <label class="np-label" for="clone-password">Password or access token</label>
                <input
                  id="clone-password"
                  class="cred-input"
                  autocomplete="current-password"
                  placeholder="••••••••••••"
                  type="password"
                  bind:value={clonePassword}
                />
              </div>
            </div>
          {/if}

          {#if cloneError}
            <output class="field-error">{cloneError}</output>
          {/if}

          <button class="go" disabled={cloneDisabled} type="submit">
            {#if cloning}
              Cloning…{:else}Clone &amp; open{/if}
          </button>
        </form>
      {/if}
    {/if}
  </div>
</section>

<style>
  /* ── Get started — one card, three tabbed ways in. ── */
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

  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
    animation: tab-in 240ms var(--ease);
  }

  @keyframes tab-in {
    from {
      opacity: 0%;
      translate: 0 4px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .panel {
      animation: none;
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

  /* A "Location"-style group row: folder icon + its contents in one field. */
  .np-loc {
    display: flex;
    flex-wrap: wrap;
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
    /* Wide enough for its own "project-name" placeholder (12ch + padding) so
       the hint never clips yet shares the root's line; typing scrolls, and
       only a genuinely long root wraps the name below. */
    --np-name-padding: 6px;

    flex: 1 1 calc(12ch + 2 * var(--np-name-padding));
    min-inline-size: calc(12ch + 2 * var(--np-name-padding));
    padding: var(--np-name-padding);
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 13px;
  }

  /* A full-width monospace path/URL input. Bare inside a `.np-loc` group row;
     `framed` gives the standalone one the field chrome itself. */
  .path-input {
    flex: 1;
    min-inline-size: 0;
    padding: 6px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 13px;

    &.framed {
      padding: 9px 12px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-medium);
      background: var(--surface-2);
    }
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

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    align-items: center;
  }

  .go {
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

  /* Quiet text-button escape hatch beside the primary action. */
  .temp-link {
    padding: 6px 0;
    border: none;
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: color 150ms var(--ease);

    &:hover {
      color: var(--primary);
      filter: none;
    }
  }

  /* Outlined secondary action (Re-check). */
  .ghost {
    padding: 9px 16px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition:
      color 150ms var(--ease),
      border-color 150ms var(--ease);

    &:hover {
      border-color: var(--primary);
      color: var(--primary);
      filter: none;
    }
  }

  /* Tonal warning surface: git missing, or the SSH→HTTPS credential fallback. */
  .warn-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    border: 1px solid var(--warning);
    border-radius: var(--radius-medium);
    background: var(--warning-wash);

    .warn-head {
      display: flex;
      gap: 8px;
      align-items: flex-start;
      margin: 0;
      color: var(--on-surface);
      font-size: 13px;
      line-height: 1.45;
    }

    .warn-ico {
      display: inline-flex;
      flex: none;
      margin-block-start: 1px;
      color: var(--warning);
    }
  }

  .cred-input {
    padding: 7px 10px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-small);
    background: var(--surface-1);
    color: var(--on-surface);
    font: inherit;
    font-size: 13px;
  }

  .browse {
    flex: none;
    padding: 6px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
    transition:
      color 150ms var(--ease),
      border-color 150ms var(--ease);

    &:hover {
      border-color: var(--primary);
      color: var(--primary);
      filter: none;
    }
  }

  /* Root select — a native-popover custom select, like the editor selects.
     Sized to its content (no grow): a short root shares one line with the
     name input; only a genuinely long one wraps the row. */
  .root-sel {
    position: relative;
    flex: 0 1 auto;
    min-inline-size: 0;
  }

  .root-trigger {
    display: inline-flex;
    gap: 6px;
    align-items: center;

    /* Full width of the row's free space — the chosen root prints in full
       (ellipsis only at the row's own edge, never a hard cap). */
    max-inline-size: 100%;
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
      filter: none;
    }

    /* The chosen root prints in full — a long path wraps (the row is
       wrap-enabled) rather than ellipsizing behind a clipped tail. */
    .root-current {
      text-align: start;
      overflow-wrap: anywhere;
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
    min-inline-size: anchor-size(inline);

    /* Anchored to the whole Location field (not the trigger): it opens flush
       with the field's start — the folder icon — and is at least field-wide. */
    position-area: block-end span-inline-end;

    .root-opt {
      justify-content: space-between;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;
    }

    /* The jump-to-add action — tinted primary so it reads as an action among
       the plain root values, hairline-separated from them when any exist. */
    .root-new {
      gap: 8px;
      color: var(--primary);
      font-weight: 600;
      font-size: 12px;
    }

    .separated {
      margin-block-start: 4px;
      padding-block-start: 4px;
      border-block-start: 1px solid var(--outline);
    }
  }
</style>
