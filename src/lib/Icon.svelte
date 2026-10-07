<script lang="ts" module>
  // One .svg file per icon lives in ./icons — the authoritative home for each
  // glyph (no hardcoded path strings). Vite inlines every file as a raw string at
  // build time; we render its inner markup (the <path>/<circle> body) inside the
  // shared <svg> wrapper below, so all icons keep one size/stroke and the files
  // stay valid standalone SVGs. To add an icon: drop a `<name>.svg` in ./icons and
  // add its name to ICON_NAMES.
  //
  // ICON_NAMES is the type's single source of truth — a glob's keys are only
  // `string` to the type system, so the name union can't be derived from them.
  const ICON_NAMES = [
    "activity", "alert", "android", "androidstudio", "angular", "antigravity", "astro",
    "batch", "bolt", "branch", "build", "c",
    "caseSensitive", "check", "checks", "chevronDown", "chevronUp", "claude", "clion",
    "clock", "close", "code", "codex", "columns",
    "copilot", "cplusplus", "csharp", "css", "csv", "cursor", "dart", "docker", "elixir",
    "env", "external", "feed", "folder", "folderPlus", "git", "github", "go", "goland",
    "gradle", "graphql", "grip", "grok", "haskell",
    "history", "html", "idea", "image", "java", "javascript", "json", "kotlin", "laravel",
    "license", "link", "llvm", "lock", "lua", "markdown", "maximize", "minimize", "monitor", "moon", "more", "npm",
    "opencode", "pencil", "perl", "php", "phpstorm", "plus", "powershell", "prisma", "pycharm", "python", "r", "rails", "react",
    "refresh", "regex", "rider", "ruby", "rubymine", "rust", "rustrover", "scala", "search", "send",
    "shader", "shell", "sliders", "sparkles", "sql", "star", "sublime", "sun", "svelte", "swap", "swift",
    "terminal", "text", "toml", "trash", "typescript", "visualstudio", "vscode", "vue", "wallet",
    "webstorm", "wholeWord", "window", "windowPlus", "xml", "yaml", "zed", "zig"
  ] as const;
  export type IconName = (typeof ICON_NAMES)[number];

  const files = import.meta.glob<string>("./icons/*.svg", {
    query: "?raw",
    import: "default",
    eager: true
  });

  // Drop each file's outer <svg …> wrapper — the template supplies it — leaving
  // just the body to inject. No regex: the first '>' closes the opening tag (SVG
  // path data and attribute values never contain one).
  function iconBody(svg: string): string {
    const bodyStart = svg.indexOf(">") + 1;
    const bodyEnd = svg.lastIndexOf("</svg>");
    return svg.slice(bodyStart, bodyEnd).trim();
  }

  function iconName(path: string): string {
    const base = path.slice(path.lastIndexOf("/") + 1);
    return base.slice(0, base.indexOf(".svg"));
  }

  const ICONS: Record<string, string> = Object.fromEntries(
    Object.entries(files).map(([path, svg]) => [iconName(path), iconBody(svg)])
  );
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
  stroke-width="1.9"
  viewBox="0 0 24 24"
  width={size}
>{@html ICONS[name] ?? ""}</svg>

<style>
  .icon {
    vertical-align: -0.15em;
    flex: none;
    block-size: 1em;
    inline-size: 1em;
  }
</style>
