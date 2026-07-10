// Shell-owned header for the side panel. Each panel publishes its live count and
// (optionally) a refresh action here; the single header in App's <aside> renders
// them, so there is one header instead of one per panel (DRY — the panels own
// only their scroll body). SoC: cross-component state lives in lib/stores.

type PanelHeader = {
  count: number | null;
  refresh: (() => void) | null;
};

const header = $state<PanelHeader>({
  count: null,
  refresh: null
});

/** Publish the active panel's header bits (call from a panel `$effect`). */
export function setPanelHeader({ count, refresh }: PanelHeader): void {
  header.count = count;
  header.refresh = refresh;
}

/** The active panel's count, or `null` when it doesn't show one. */
export function panelCount(): number | null {
  return header.count;
}

/** The active panel's refresh action, or `null` when it isn't refreshable. */
export function panelRefresh(): (() => void) | null {
  return header.refresh;
}
