<script lang="ts">
  import Icon from "@/lib/Icon.svelte";

  // The root select - trigger + anchored menu incl. the jump-to-add action.
  // The New and Clone location rows each mount one (both panels stay in the
  // DOM for the pure-CSS height glide), so the menu id is a parameter; the
  // hosting row declares `anchor-name: --<menuId>` to pair with it. The menu
  // hides imperatively BEFORE handing off to `onnewroot`, because the
  // declarative popovertargetaction="hide" would run after the click handler
  // and cancel the smooth scroll to the add field.
  let { roots, menuId, onnewroot, label, value = $bindable("") }: {
    roots: string[];
    menuId: string;
    onnewroot: () => void;
    /** How the trigger names the chosen root, when not its full path. */
    label?: string;
    value?: string;
  } = $props();

  const triggerLabel = $derived(label || value || "Choose a root…");
</script>

<span class="root-selector menu-host">
  <button
    class="root-trigger menu-trigger"
    aria-label="Root folder"
    popovertarget={menuId}
    type="button"
  >
    <span class="root-current">{triggerLabel}</span>
    <span class="caret" aria-hidden="true">▾</span>
  </button>
  <ul id={menuId} style:position-anchor={`--${menuId}`} class="menu root-menu popover-menu" popover>
    {#each roots as root (root)}
      {@const isPicked = value === root}
      <li>
        <button
          class="menu-item root-option"
          class:picked={isPicked}
          aria-current={isPicked}
          onclick={() => (value = root)}
          popovertarget={menuId}
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
        class="menu-item root-new"
        onclick={e => {
          e.currentTarget.closest("ul")?.hidePopover();
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

<style>
  /* Root select - a native-popover custom select, like the editor selects.
     Sized to its content (no grow): a short root shares one line with the
     name input; only a genuinely long one wraps the row. */
  .root-selector {
    position: relative;
    flex: 0 1 auto;
    min-inline-size: 0;
  }

  .root-trigger {
    display: inline-flex;
    gap: 6px;
    align-items: center;

    /* Full width of the row's free space - the chosen root prints in full
       (ellipsis only at the row's own edge, never a hard cap). */
    max-inline-size: 100%;
    padding: 6px 2px;
    border: none;
    background: transparent;
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 0.8125rem;
    cursor: pointer;
    transition: color 150ms var(--ease);

    &:hover {
      color: var(--primary);
      filter: none;
    }

    /* The chosen root prints in full - a long path wraps (the row is
       wrap-enabled) rather than ellipsizing behind a clipped tail. */
    .root-current {
      text-align: start;
      overflow-wrap: anywhere;
    }

    .caret {
      display: inline-block;
      flex: none;
      font-size: 0.5625rem;
      opacity: 70%;
      transition: rotate 150ms var(--ease);
    }
  }

  /* Open state - the field's own extra on top of the shared `.menu-host` active
     look (border/background/caret, from theme.css): the trigger text tints
     primary while the menu is open. Pure CSS, field-scoped. */
  .root-selector:has(.root-menu:popover-open) .root-trigger {
    color: var(--primary);
  }

  .root-menu {
    min-inline-size: anchor-size(inline);

    /* Anchored to the whole Location field (not the trigger): it opens flush
       with the field's start - the folder icon - and is at least field-wide. */
    position-area: block-end span-inline-end;

    .root-option {
      justify-content: space-between;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 0.75rem;
    }

    /* The jump-to-add action - tinted primary so it reads as an action among
       the plain root values, hairline-separated from them when any exist. */
    .root-new {
      gap: 8px;
      color: var(--primary);
      font-weight: 600;
      font-size: 0.75rem;
    }

    .separated {
      margin-block-start: 4px;
      padding-block-start: 4px;
      border-block-start: 1px solid var(--outline);
    }
  }
</style>
