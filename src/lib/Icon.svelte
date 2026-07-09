<script lang="ts" module>
  // Lucide-style stroke icons, one path per name (a path may hold several
  // subpaths). Add an entry here to make a new icon available everywhere.
  const PATHS: Record<string, string> = {
    folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
    terminal: "M5 6l5 6-5 6 M13 18h6",
    code: "M9 6l-5 6 5 6 M15 6l5 6-5 6",
    feed: "M4 7h16 M4 12h16 M4 17h10",
    git: "M6 4a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M6 16a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M18 8a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M6 8v8 M18 12a6 6 0 0 1-6 4",
    sliders: "M4 7h16 M4 12h16 M4 17h16 M9 5v4 M15 10v4 M7 15v4",
    plus: "M12 5v14 M5 12h14",
    trash: "M4 7h16 M9 7V5h6v2 M6 7l1 13h10l1-13",
    external: "M14 4h6v6 M20 4l-9 9 M5 6v13h13v-6",
    refresh: "M20 12a8 8 0 1 1-2.3-5.6 M20 4v4h-4"
  };

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  const { name, size = 16 }: {
    name: IconName;
    size?: number;
  } = $props();
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
  <path d={PATHS[name]} />
</svg>

<style>
  .icon {
    flex: none;
    inline-size: 1em;
    block-size: 1em;
    vertical-align: -0.15em;
  }
</style>
