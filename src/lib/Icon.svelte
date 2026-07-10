<script lang="ts" module>
  // Lucide-style stroke icons. Each entry holds a `path` (one string, may pack
  // several subpaths separated by spaces) and/or `circles` ([cx, cy, r]); a few
  // marks (the kebab) are filled dots. Add an entry here to make a new icon
  // available everywhere — one authoritative home for the icon set (DRY).
  type IconDef = {
    path?: string;
    circles?: readonly (readonly [number, number, number])[];
    filled?: boolean;
  };

  const ICONS = {
    folder: { path: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" },
    folderPlus: { path: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M12 10v6 M9 13h6" },
    terminal: { path: "M5 6l5 6-5 6 M13 18h6" },
    code: { path: "M9 6l-5 6 5 6 M15 6l5 6-5 6" },
    feed: { path: "M4 7h16 M4 12h16 M4 17h10" },
    activity: { path: "M22 12h-4l-3 9L9 3l-3 9H2" },
    git: { path: "M6 4a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M6 16a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M18 8a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M6 8v8 M18 12a6 6 0 0 1-6 4" },
    branch: { path: "M6 3v12 M18 9a9 9 0 0 1-9 9", circles: [[18, 6, 3], [6, 18, 3]] },
    checks: { path: "M3 7l2 2 4-4 M3 17l2 2 4-4 M13 6h8 M13 12h8 M13 18h8" },
    sliders: { path: "M4 7h16 M4 12h16 M4 17h16 M9 5v4 M15 10v4 M7 15v4" },
    plus: { path: "M12 5v14 M5 12h14" },
    close: { path: "M18 6 6 18 M6 6l12 12" },
    trash: { path: "M4 7h16 M9 7V5h6v2 M6 7l1 13h10l1-13" },
    pencil: { path: "M12 20h9 M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" },
    swap: { path: "M16 3l4 4-4 4 M20 7H4 M8 21l-4-4 4-4 M4 17h16" },
    external: { path: "M14 4h6v6 M20 4l-9 9 M5 6v13h13v-6" },
    refresh: { path: "M20 12a8 8 0 1 1-2.3-5.6 M20 4v4h-4" },
    history: { path: "M3 12a9 9 0 1 0 3-6.7L3 8 M3 3v5h5 M12 7v5l4 2" },
    columns: { path: "M3 4h18v16H3z M12 4v16" },
    maximize: { path: "M8 3H5a2 2 0 0 0-2 2v3 M16 3h3a2 2 0 0 1 2 2v3 M8 21H5a2 2 0 0 1-2-2v-3 M16 21h3a2 2 0 0 0 2-2v-3" },
    minimize: { path: "M3 8h3a2 2 0 0 0 2-2V3 M21 8h-3a2 2 0 0 1-2-2V3 M3 16h3a2 2 0 0 1 2 2v3 M21 16h-3a2 2 0 0 0-2 2v3" },
    more: { circles: [[5, 12, 1.4], [12, 12, 1.4], [19, 12, 1.4]], filled: true },
    star: { path: "M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z" },
    sparkles: { path: "M12 3l1.9 5.3a2 2 0 0 0 1.8 1.8L21 12l-5.3 1.9a2 2 0 0 0-1.8 1.8L12 21l-1.9-5.3a2 2 0 0 0-1.8-1.8L3 12l5.3-1.9a2 2 0 0 0 1.8-1.8z M19 4v3 M17.5 5.5h3" }
  } as const satisfies Record<string, IconDef>;

  export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
  const { name, size = 16 }: {
    name: IconName;
    size?: number;
  } = $props();

  const def = $derived(ICONS[name] as IconDef);
</script>

<svg
  class="icon"
  aria-hidden="true"
  fill="none"
  height={size}
  stroke="currentColor"
  stroke-linecap="round"
  stroke-linejoin="round"
  stroke-width="2"
  viewBox="0 0 24 24"
  width={size}
>
  {#if def.path}
    <path d={def.path} />
  {/if}
  {#each def.circles ?? [] as [cx, cy, r] (`${cx}-${cy}`)}
    <circle {cx} {cy} {r} fill={def.filled ? "currentColor" : "none"} stroke={def.filled ? "none" : "currentColor"} />
  {/each}
</svg>

<style>
  .icon {
    flex: none;
    vertical-align: -0.15em;
    block-size: 1em;
    inline-size: 1em;
  }
</style>
