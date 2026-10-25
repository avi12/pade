/** Attachment factory (`{@attach popoverOpenWhen(isVisible)}`) for a
 *  `popover="manual"` element whose visibility follows state rather than a
 *  `popovertarget` click - a typeahead's listbox opens on typing. Svelte re-runs
 *  the attachment whenever `isOpen` changes; the `:popover-open` check keeps it
 *  idempotent. */
export function popoverOpenWhen(isOpen: boolean) {
  return (elPopover: HTMLElement) => {
    const isAlreadyThere = elPopover.matches(":popover-open") === isOpen;
    if (isAlreadyThere) {
      return;
    }

    elPopover.togglePopover(isOpen);
  };
}
