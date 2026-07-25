// Shared lazy project-kind detection for project rows. One observer watches every
// consumer, visible uncached paths coalesce into one backend batch, and one
// reactive cache updates AppMenu/picker surfaces without per-row IPC calls.

import { ide } from "@/lib/bridge";
import { normalizePath } from "@/lib/paths";
import { SvelteMap } from "svelte/reactivity";

const CACHE_LIMIT = 256;
const kinds = new SvelteMap<string, string | null>();
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- request bookkeeping, never rendered
const originalPaths = new Map<string, string>();
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

function publish({ key, kind }: {
  key: string;
  kind: string | null;
}): void {
  kinds.set(key, kind);

  if (kinds.size <= CACHE_LIMIT) {
    return;
  }

  const oldest = kinds.keys().next().value;
  if (oldest !== undefined) {
    kinds.delete(oldest);
    originalPaths.delete(oldest);
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
  loading = true;
  try {
    const detected = await ide.projectKinds(paths);
    for (const [index, key] of keys.entries()) {
      const path = paths[index];
      publish({
        key,
        kind: detected[path] ?? null
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

function enqueue({ key, path }: {
  key: string;
  path: string;
}): void {
  if (kinds.has(key)) {
    return;
  }

  if (!originalPaths.has(key)) {
    originalPaths.set(key, path);
  }

  pending.add(key);
  scheduleFlush();
}

/** Queue one path through the shared batch. Exposed separately from the DOM
 * attachment so non-visual consumers and unit tests use the identical loader. */
export function requestProjectKind(path: string): void {
  enqueue({
    key: normalizePath(path),
    path
  });
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

function projectKindObserver(): IntersectionObserver {
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
        requestProjectKind(originalPaths.get(state.key) ?? state.key);
      }
    }
  });
  return observer;
}

/** Cached project kind: `undefined` while unresolved, `null` when probed empty. */
export function projectKind(path: string): string | null | undefined {
  return kinds.get(normalizePath(path));
}

/** Probe one project immediately and publish through the shared cache. This is
 * used by non-visual evidence flows that must know when a marker first exists. */
export async function refreshProjectKind(path: string): Promise<string | null | undefined> {
  let detected: Record<string, string>;
  try {
    detected = await ide.projectKinds([path]);
  } catch {
    return undefined;
  }

  const kind = detected[path] ?? null;
  publish({
    key: normalizePath(path),
    kind
  });
  return kind;
}

/** Shared attachment that detects a project only when its row becomes visible. */
export function observeProjectKind({ path }: { path: string }) {
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
      requestProjectKind(path);
    } else {
      projectKindObserver().observe(element);
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
