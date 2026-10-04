// Shared lazy, per-project lookups for project rows (the language icon, the
// branch chip). One IntersectionObserver per lookup watches every row; paths
// whose rows become visible coalesce into one backend batch, and one reactive
// cache feeds every surface — so a list of any length only ever asks the backend
// about the rows actually on screen, never one call per row and never the whole
// list up front.

import { normalizePath } from "@/lib/paths";
import { SvelteMap } from "svelte/reactivity";

/** One lazily-loaded, visibility-driven lookup keyed by project path. */
export interface VisiblePathBatch<Value> {
  /** Cached value: `undefined` while unresolved, `null` when probed empty. */
  value(path: string): Value | null | undefined;
  /** Queue one path through the shared batch (non-visual consumers, tests). */
  request(path: string): void;
  /** Probe one path immediately, bypassing the batch, and publish the result. */
  refresh(path: string): Promise<Value | null | undefined>;
  /** Attachment that requests a path only once its row becomes visible. */
  observe(args: { path: string }): (element: Element) => () => void;
  /** Forget every cached value and re-request the rows still on screen — for a
   *  value that changes under us (a branch switch) without an event per path. */
  invalidate(): void;
}

export function createVisiblePathBatch<Value>({ load, cacheLimit }: {
  /** Resolve a batch of paths; a path missing from the result is probed empty. */
  load: (paths: string[]) => Promise<Record<string, Value>>;
  /** Oldest entries are evicted beyond this many cached paths. */
  cacheLimit: number;
}): VisiblePathBatch<Value> {
  const values = new SvelteMap<string, Value | null>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- request bookkeeping, never rendered
  const originalPaths = new Map<string, string>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- request ordering, never rendered
  const latestRequests = new Map<string, symbol>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- request ordering, never rendered
  const refreshRequests = new Map<string, symbol>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- request bookkeeping, never rendered
  const pending = new Set<string>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- observer bookkeeping, never rendered
  const visibleCounts = new Map<string, number>();
  const observed = new WeakMap<Element, {
    key: string;
    visible: boolean;
  }>();
  let observer: IntersectionObserver | undefined;
  let loading = false;
  let flushQueued = false;

  function publish({ key, value }: {
    key: string;
    value: Value | null;
  }): void {
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

  async function flushPending(): Promise<void> {
    flushQueued = false;

    if (loading || pending.size === 0) {
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
    loading = true;
    try {
      const loaded = await load(paths);
      for (const [index, key] of keys.entries()) {
        if (latestRequests.get(key) !== requests[index]) {
          continue;
        }

        publish({
          key,
          value: loaded[paths[index]] ?? null
        });
      }
    } catch {
      // Leave failures unresolved so a later mount can retry.
    } finally {
      loading = false;
    }

    if (pending.size > 0) {
      scheduleFlush();
    }
  }

  function scheduleFlush(): void {
    if (flushQueued) {
      return;
    }

    flushQueued = true;
    queueMicrotask(flushPending);
  }

  function request(path: string): void {
    const key = normalizePath(path);
    if (values.has(key) || refreshRequests.has(key)) {
      return;
    }

    if (!originalPaths.has(key)) {
      originalPaths.set(key, path);
    }

    pending.add(key);
    scheduleFlush();
  }

  function adjustVisibility({ key, becameVisible }: {
    key: string;
    becameVisible: boolean;
  }): void {
    const count = visibleCounts.get(key) ?? 0;
    const next = Math.max(0, count + (becameVisible ? 1 : -1));
    if (next === 0) {
      visibleCounts.delete(key);
    } else {
      visibleCounts.set(key, next);
    }
  }

  function rowObserver(): IntersectionObserver {
    observer ??= new IntersectionObserver(entries => {
      for (const entry of entries) {
        const state = observed.get(entry.target);
        if (!state || state.visible === entry.isIntersecting) {
          continue;
        }

        state.visible = entry.isIntersecting;
        adjustVisibility({
          key: state.key,
          becameVisible: entry.isIntersecting
        });

        if (entry.isIntersecting) {
          request(originalPaths.get(state.key) ?? state.key);
        }
      }
    });
    return observer;
  }

  function value(path: string): Value | null | undefined {
    return values.get(normalizePath(path));
  }

  async function refresh(path: string): Promise<Value | null | undefined> {
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

    const result = loaded[path] ?? null;
    if (latestRequests.get(key) !== refreshRequest) {
      return value(path);
    }

    publish({
      key,
      value: result
    });
    return result;
  }

  function observe({ path }: { path: string }) {
    return (element: Element) => {
      const key = normalizePath(path);
      originalPaths.set(key, path);
      const state = {
        key,
        visible: false
      };
      observed.set(element, state);

      if (typeof IntersectionObserver === "undefined") {
        state.visible = true;
        adjustVisibility({
          key,
          becameVisible: true
        });
        request(path);
      } else {
        rowObserver().observe(element);
      }

      return () => {
        observer?.unobserve(element);

        if (state.visible) {
          adjustVisibility({
            key,
            becameVisible: false
          });
        }

        observed.delete(element);
      };
    };
  }

  function invalidate(): void {
    values.clear();
    for (const key of visibleCounts.keys()) {
      request(originalPaths.get(key) ?? key);
    }
  }

  return {
    value,
    request,
    refresh,
    observe,
    invalidate
  };
}
