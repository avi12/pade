<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import type { Snippet } from "svelte";

  // Shared confirmation prompt — an in-app themed card in place of the OS popup, so
  // a destructive prompt carries rich body content and can stay open to show why
  // the action failed. Two shells around one card:
  //   • default — a modal <dialog>: showModal() gives Esc-to-dismiss, the focus
  //     trap and the ::backdrop scrim for free (semantic HTML over a hand-rolled
  //     trap).
  //   • `nested` — a full-viewport `popover` opened inside an already-open menu
  //     popover, so that menu stays visible (dimmed) behind the prompt instead of
  //     being force-closed (showModal() closes every open popover). The scrim IS
  //     the popover element, so a click on the dim cancels (no click-through to
  //     the menu) and Esc light-dismisses → oncancel via `ontoggle`.
  // The caller owns the work: it passes `busy` while its promise is in flight and
  // `error` when it rejects.
  const {
    title,
    icon,
    confirmLabel,
    busyLabel,
    danger = false,
    busy = false,
    error = null,
    nested = false,
    onconfirm,
    oncancel,
    children
  }: {
    title: string;
    icon: IconName;
    confirmLabel: string;
    /** Shown on the confirm button while `busy` (defaults to confirmLabel). */
    busyLabel?: string;
    /** Paints the glyph and the confirm button in the critical role. */
    danger?: boolean;
    busy?: boolean;
    error?: string | null;
    /** Render as a popover nested in an open menu (keeps that menu visible behind)
     *  rather than a top-level modal <dialog>. */
    nested?: boolean;
    onconfirm: () => Promise<void> | void;
    oncancel: () => void;
    children: Snippet;
  } = $props();

  const actionLabel = $derived(busy && busyLabel ? busyLabel : confirmLabel);

  let dialogElement = $state<HTMLDialogElement | null>(null);
  let scrimElement = $state<HTMLDialogElement | null>(null);

  // Lift onto the top layer once mounted (showModal throws on an open dialog).
  $effect(() => {
    if (dialogElement && !dialogElement.open) {
      dialogElement.showModal();
    }
  });

  // The nested popover doesn't move focus on show the way showModal does, so land
  // it on Cancel — the safe default for a destructive prompt.
  $effect(() => {
    if (scrimElement && !scrimElement.matches(":popover-open")) {
      scrimElement.showPopover();
      scrimElement.querySelector<HTMLElement>(".cancel")?.focus();
    }
  });
</script>

{#snippet card()}
  <div class="glyph" aria-hidden="true"><Icon name={icon} size={20} /></div>
  <h2 id="confirm-title">{title}</h2>

  <div class="body">{@render children()}</div>

  {#if error}
    <p class="error" role="alert"><Icon name="alert" size={15} /><span>{error}</span></p>
  {/if}

  <footer>
    <button class="cancel" disabled={busy} onclick={oncancel} type="button">Cancel</button>
    <button class="confirm" disabled={busy} onclick={onconfirm} type="button">
      {#if busy}
        <span class="spinner"><Icon name="refresh" size={15} /></span>
      {/if}
      {actionLabel}
    </button>
  </footer>
{/snippet}

{#if nested}
  <!-- A <dialog> shown as a nested popover — not showModal(), which would force-close
       the menu popover behind it. It fills the viewport as its own scrim (a fit-content
       popover's ::backdrop leaks clicks to the menu behind), so a click on the dim
       cancels without reaching the menu; Esc light-dismisses → oncancel via ontoggle.
       Being a <dialog>, the backdrop click needs no faux-interactive role. -->
  <dialog
    bind:this={scrimElement}
    class="scrim"
    class:danger
    popover="auto"
    aria-labelledby="confirm-title"
    ontoggle={e => {
      if ((e as ToggleEvent).newState === "closed" && !busy) {
        oncancel();
      }
    }}
    onclick={e => {
      if (e.target === scrimElement && !busy) {
        oncancel();
      }
    }}
  >
    <div class="dialog" class:danger>{@render card()}</div>
  </dialog>
{:else}
  <dialog
    bind:this={dialogElement}
    class="dialog"
    class:danger
    aria-labelledby="confirm-title"
    oncancel={e => {
      // Esc (the native "cancel" event) routes to the parent, so the caller's
      // state stays the single source of truth for whether the dialog is mounted.
      e.preventDefault();

      if (!busy) {
        oncancel();
      }
    }}
    onclick={e => {
      // A modal <dialog>'s hit area spans the viewport: a click whose target is
      // the dialog itself landed on the ::backdrop, not on the card.
      if (e.target === dialogElement && !busy) {
        oncancel();
      }
    }}
  >{@render card()}</dialog>
{/if}

<style>
  /* The card itself: showModal() puts it on the top layer and paints ::backdrop
     as the scrim, so no separate scrim element is needed. */
  .dialog {
    inline-size: min(460px, calc(100% - 48px));
    margin: auto;
    padding: 24px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-extra-large);
    background: var(--surface-1);
    color: var(--on-surface);
    outline: none;
    box-shadow: 0 32px 80px var(--shadow-color);
    animation: pop-in 240ms var(--spring);

    &::backdrop {
      background: color-mix(in sRGB, var(--shadow-color) 70%, hsl(214deg 40% 4% / 55%));
      animation: fadein 160ms var(--ease);
    }
  }

  /* Nested shell: a <dialog>-as-popover that fills the viewport and IS its own scrim,
     so the menu behind stays visible (dimmed) yet un-clickable and there's no
     fit-content ::backdrop for clicks to leak through. `display` is gated on
     :popover-open (beating the UA `dialog:not([open])` display:none) so it centres
     the card only while shown. */
  .scrim {
    position: fixed;
    inset: 0;
    max-inline-size: none;
    max-block-size: none;
    inline-size: 100%;
    block-size: 100%;
    margin: 0;
    padding: 24px;
    border: none;
    overflow: auto;
    color: inherit;
    background: color-mix(in sRGB, var(--shadow-color) 70%, hsl(214deg 40% 4% / 55%));
    animation: fadein 160ms var(--ease);

    &:popover-open {
      display: grid;
      place-items: center;
    }

    .dialog {
      margin: 0;
    }
  }

  /* Tonal disc behind the icon — critical wash for a destructive prompt. */
  .glyph {
    display: inline-flex;
    justify-content: center;
    align-items: center;
    block-size: 44px;
    inline-size: 44px;
    border-radius: 999px;
    background: var(--primary-container);
    color: var(--on-primary-container);

    .danger & {
      background: var(--critical-wash);
      color: var(--critical);
    }
  }

  h2 {
    margin-block: 16px 0;
    margin-inline: 0;
    font-weight: 700;
    font-size: 20px;
    letter-spacing: -0.01em;
    text-wrap: balance;
  }

  /* Body copy comes from the caller's snippet; only the inherited type styles
     are set here so the caller stays in charge of its own layout. */
  .body {
    margin-block-start: 8px;
    color: var(--on-surface-variant);
    font-size: 14px;
    line-height: 1.5;
  }

  .error {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin-block: 16px 0;
    margin-inline: 0;
    padding: 10px 12px;
    border-radius: var(--radius-medium);
    background: var(--critical-wash);
    color: var(--critical);
    font-size: 13px;
    line-height: 1.4;
  }

  footer {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-block-start: 24px;
  }

  button {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding-block: 10px;
    padding-inline: 20px;
    border: none;
    border-radius: 999px;
    font: inherit;
    font-weight: 700;
    font-size: 13px;
    cursor: pointer;
    transition:
      background 140ms var(--ease),
      filter 140ms var(--ease);

    &:disabled {
      opacity: 60%;
      cursor: default;
    }
  }

  .cancel {
    background: transparent;
    color: var(--on-surface-variant);

    &:hover:not(:disabled) {
      background: var(--surface-2);
      color: var(--on-surface);
    }
  }

  .confirm {
    background: var(--primary);
    color: var(--on-primary);

    &:hover:not(:disabled) {
      filter: brightness(1.06);
    }

    .danger & {
      background: var(--critical);
      color: var(--surface);
    }
  }

  .spinner {
    display: inline-flex;
    animation: spin 900ms linear infinite;
  }
</style>
