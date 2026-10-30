<script lang="ts" module>
  // One .svg file per icon lives under ./icons, grouped into semantic subfolders
  // (agents, editors, languages, actions, ...) - the authoritative home for each
  // glyph (no hardcoded path strings). Vite inlines every file as a raw string at
  // build time; we render its inner markup (the <path>/<circle> body) inside the
  // shared <svg> wrapper below, so all icons keep one size/stroke and the files
  // stay valid standalone SVGs. To add an icon: drop a `<name>.svg` in the
  // subfolder it belongs to and add its name to ICON_NAMES.
  //
  // ICON_NAMES is the type's single source of truth - a glob's keys are only
  // `string` to the type system, so the name union can't be derived from them.
  const ICON_NAMES = [
    "activity", "alert", "android", "androidstudio", "angular", "antigravity", "astro",
    "batch", "bolt", "branch", "build", "c",
    "caseSensitive", "check", "checks", "chevronDown", "chevronUp", "claude", "clion",
    "clock", "close", "code", "codex", "columns",
    "copilot", "cplusplus", "csharp", "css", "csv", "cursor", "dart", "docker", "elixir",
    "env", "external", "feed", "folder", "folderOpen", "folderPlus", "git", "github", "go", "goland",
    "gradle", "graphql", "grip", "grok", "haskell",
    "history", "html", "idea", "image", "java", "javascript", "json", "kotlin", "laravel",
    "license", "link", "llvm", "lock", "lua", "markdown", "maximize", "minimize", "monitor", "moon", "more", "npm",
    "opencode", "pause", "pencil", "perl", "php", "phpstorm", "plus", "powershell", "prisma", "pycharm", "python", "r", "rails", "react",
    "refresh", "regex", "rider", "ruby", "rubymine", "rust", "rustrover", "scala", "search", "send",
    "shader", "shell", "sliders", "sparkles", "sql", "star", "stop", "sublime", "sun", "svelte", "swap", "swift",
    "terminal", "text", "toml", "trash", "typescript", "visualstudio", "vscode", "vue", "wallet",
    "webstorm", "wholeWord", "window", "windowPlus", "xml", "yaml", "zed", "zig"
  ] as const;
  export type IconName = (typeof ICON_NAMES)[number];

  const SVG_EXTENSION = ".svg";
  const SVG_CLOSING_TAG = "</svg>";

  const ICON_FILES = import.meta.glob<string>("./icons/**/*.svg", {
    query: "?raw",
    import: "default",
    eager: true
  });

  // Drop each file's outer <svg …> wrapper - the template supplies it - leaving
  // just the body to inject. No regex: the first '>' closes the opening tag (SVG
  // path data and attribute values never contain one).
  function iconBody(svg: string) {
    const bodyStart = svg.indexOf(">") + 1;
    return svg.slice(bodyStart, svg.lastIndexOf(SVG_CLOSING_TAG)).trim();
  }

  function iconName(path: string) {
    const base = path.slice(path.lastIndexOf("/") + 1);
    return base.slice(0, base.indexOf(SVG_EXTENSION));
  }

  // A name is the file's base name wherever it sits under ./icons, so two files
  // sharing one would silently shadow each other - refuse to load instead.
  function iconsByName(files: Record<string, string>) {
    const icons: Record<string, string> = {};
    for (const [path, svg] of Object.entries(files)) {
      const name = iconName(path);
      if (Object.hasOwn(icons, name)) {
        throw new Error(`Duplicate icon name "${name}": ${path} clashes with another file in ./icons`);
      }

      icons[name] = iconBody(svg);
    }

    return icons;
  }

  const ICONS = iconsByName(ICON_FILES);
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
