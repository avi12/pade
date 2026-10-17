// The dashboard's sort choice, remembered on this machine so it opens the way it
// was left. A view preference of the one dashboard window, not a setting any
// other surface reads, so it lives with the window rather than in the app prefs.

import { BoardSort } from "@/lib/dashboard/agent-board";

const SORT_STORAGE_KEY = "pade.dashboard-sort";

/** The remembered sort, else window order. */
export function rememberedSort() {
  try {
    const parsed = BoardSort.safeParse(localStorage.getItem(SORT_STORAGE_KEY));
    return parsed.success ? parsed.data : BoardSort.enum.windows;
  } catch (error) {
    console.warn("pade: could not read the dashboard's sort, using window order", error);
    return BoardSort.enum.windows;
  }
}

export function rememberSort(sort: BoardSort) {
  try {
    localStorage.setItem(SORT_STORAGE_KEY, sort);
  } catch (error) {
    console.warn("pade: could not remember the dashboard's sort", error);
  }
}
