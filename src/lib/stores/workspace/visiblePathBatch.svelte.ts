// Shared lazy, per-project lookups for project rows (the language icon, the
// branch chip). One IntersectionObserver per lookup watches every row; paths
// whose rows become visible coalesce into one backend batch, and one reactive
// cache feeds every surface - so a list of any length only ever asks the backend
// about the rows actually on screen, never one call per row and never the whole
// list up front.

import { normalizePath } from "@/lib/util/paths";
import { SvelteMap } from "svelte/reactivity";

/** How many project paths each row lookup keeps cached - comfortably more than
 *  a recent-projects list shows, so scrolling back never re-asks the backend. */
export const PROJECT_ROW_CACHE_LIMIT = 256;

/** One lazily-loaded, visibility-driven lookup keyed by project path. */
export function createVisiblePathBatch<Value>({ load, cacheLimit, onVisiblePathsChange }: {
  /** Resolve a batch of paths; a path missing from the result is probed empty. */
  load: (paths: string[]) => Promise<Record<string, Value>>;
  /** Oldest entries are evicted beyond this many cached paths. */
  cacheLimit: number;
  /** Told the paths on screen whenever a row enters or leaves the screen - for
   *  a lookup that keeps a live watch on exactly those rows. */
  onVisiblePathsChange?: (paths: string[]) => void;
}) {
  const values = new SvelteMap<string, Value | null>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- request bookkeeping, never rendered
  const originalPaths = new Map<string, string>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- request ordering, never rendered
  const latestRequests = new Map<string, symbol>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- request ordering, never rendered
  const refreshRequests = new Map<string, symbol>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- request bookkeeping, never rendered
  const pending = new Set<string>();
  // Cached values still shown, but re-asked the next time their row is on screen.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- request bookkeeping, never rendered
  const staleKeys = new Set<string>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- observer bookkeeping, never rendered
  const visibleCounts = new Map<string, number>();
  const observed = new WeakMap<Element, {
    key: string;
    isVisible: boolean;
  }>();
  let observer: IntersectionObserver | undefined;
  let isLoading = false;
  let isFlushQueued = false;

  function publish({ key, value }: {
    key: string;
    value: Value | null;
  }) {
    staleKeys.delete(key);
    values.set(key, value);

    if (values.size <= cacheLimit) {
      return;
    }

    const oldest = values.keys().next().value;
    if (oldest !== undefined) {
      values.delete(oldest);
      originalPaths.delete(oldest);
      latestRequests.delete(oldest);
    }
  }

  async function flushPending() {
    isFlushQueued = false;

    if (isLoading || pending.size === 0) {
      return;
    }

    const keys = [...pending];
    pending.clear();
    const paths = keys.map(key => originalPaths.get(key) ?? key);
    const requests = keys.map(key => {
      const request = Symbol(key);
      latestRequests.set(key, request);
      return request;
    });
    isLoading = true;
    try {
      const loaded = await load(paths);
      for (const [i, key] of keys.entries()) {
        if (latestRequests.get(key) !== requests[i]) {
          continue;
        }

        publish({
          key,
          value: loaded[paths[i]] ?? null
        });
      }
    } catch (error) {
      // Leave the paths unresolved so a later mount can retry them.
      console.warn("pade: project row lookup failed, will retry on the next mount", error);
    } finally {
      isLoading = false;
    }

    if (pending.size > 0) {
      scheduleFlush();
    }
  }

  function scheduleFlush() {
    if (isFlushQueued) {
      return;
    }

    isFlushQueued = true;
    queueMicrotask(flushPending);
  }

  /** Queue one path through the shared batch (non-visual consumers, tests). */
  function request(path: string) {
    const key = normalizePath(path);
    const isFresh = values.has(key) && !staleKeys.has(key);
    if (isFresh || refreshRequests.has(key)) {
      return;
    }

    if (!originalPaths.has(key)) {
      originalPaths.set(key, path);
    }

    pending.add(key);
    scheduleFlush();
  }

  function adjustVisibility({ key, isBecomingVisible }: {
    key: string;
    isBecomingVisible: boolean;
  }) {
    const count = visibleCounts.get(key) ?? 0;
    const next = Math.max(0, count + (isBecomingVisible ? 1 : -1));
    if (next === 0) {
      visibleCounts.delete(key);
    } else {
      visibleCounts.set(key, next);
    }

    const isOnScreenChange = (count === 0) !== (next === 0);
    if (isOnScreenChange) {
      onVisiblePathsChange?.([...visibleCounts.keys()].map(visibleKey => originalPaths.get(visibleKey) ?? visibleKey));
    }
  }

  function rowObserver() {
    observer ??= new IntersectionObserver(entries => {
      for (const entry of entries) {
        const state = observed.get(entry.target);
        if (!state || state.isVisible === entry.isIntersecting) {
          continue;
        }

        state.isVisible = entry.isIntersecting;
        adjustVisibility({
          key: state.key,
          isBecomingVisible: entry.isIntersecting
        });

        if (entry.isIntersecting) {
          request(originalPaths.get(state.key) ?? state.key);
        }
      }
    });
    return observer;
  }

  /** Cached value: `undefined` while unresolved, `null` when probed empty. */
  function value(path: string) {
    return values.get(normalizePath(path));
  }

  /** Probe one path immediately, bypassing the batch, and publish the result. */
  async function refresh(path: string) {
    const key = normalizePath(path);
    const refreshRequest = Symbol(key);
    latestRequests.set(key, refreshRequest);
    refreshRequests.set(key, refreshRequest);
    pending.delete(key);

    let loaded: Record<string, Value>;
    try {
      loaded = await load([path]);
    } catch {
      return undefined;
    } finally {
      if (refreshRequests.get(key) === refreshRequest) {
        refreshRequests.delete(key);
      }
    }

    const loadedValue = loaded[path] ?? null;
    if (latestRequests.get(key) !== refreshRequest) {
      return value(path);
    }

    publish({
      key,
      value: loadedValue
    });
    return loadedValue;
  }

  /** Attachment that requests a path only once its row becomes visible. */
  function observe({ path }: { path: string }) {
    return (element: Element) => {
      const key = normalizePath(path);
      originalPaths.set(key, path);
      const state = {
        key,
        isVisible: false
      };
      observed.set(element, state);

      const isObserverAvailable = typeof IntersectionObserver !== "undefined";
      if (isObserverAvailable) {
        rowObserver().observe(element);
      } else {
        state.isVisible = true;
        adjustVisibility({
          key,
          isBecomingVisible: true
        });
        request(path);
      }

      return () => {
        observer?.unobserve(element);

        if (state.isVisible) {
          adjustVisibility({
            key,
            isBecomingVisible: false
          });
        }

        observed.delete(element);
      };
    };
  }

  /** Forget every cached value and re-request the rows still on screen - for a
   *  value that changes under us (a branch switch) without an event per path. */
  function invalidate() {
    values.clear();
    for (const key of visibleCounts.keys()) {
      request(originalPaths.get(key) ?? key);
    }
  }

  /** Keep showing every cached value but re-ask the backend for each one, the
   *  rows on screen now and the rest once they scroll into view - for a value
   *  that may have changed (a project gaining a `tsconfig.json`) where blanking
   *  it while it reloads would only flicker. */
  function revalidate() {
    for (const key of values.keys()) {
      staleKeys.add(key);
    }
    for (const key of visibleCounts.keys()) {
      request(originalPaths.get(key) ?? key);
    }
  }

  return {
    value,
    request,
    refresh,
    observe,
    invalidate,
    revalidate
  };
}
