import type { IconName } from "@/lib/Icon.svelte";

// The editors PADE detects — mirrors the launcher ids in src-tauri/src/ide.rs, so
// no bare editor-id string leaks into the icon map (enums over magic strings).
export const IdeId = {
  VsCode: "vscode",
  Cursor: "cursor",
  WebStorm: "webstorm",
  IntelliJ: "idea",
  PyCharm: "pycharm",
  GoLand: "goland",
  RustRover: "rustrover",
  Rider: "rider",
  CLion: "clion",
  PhpStorm: "phpstorm",
  RubyMine: "rubymine",
  AndroidStudio: "androidstudio",
  Zed: "zed",
  Sublime: "sublime",
  VisualStudio: "visualstudio"
} as const;
export type IdeId = (typeof IdeId)[keyof typeof IdeId];

// Brand mark per detected editor. The JetBrains IDEs share the JetBrains family
// mark — legible at tab size and honest, where a per-product colour logo can't
// reduce to one monochrome glyph.
const IDE_ICONS: Record<string, IconName> = {
  [IdeId.VsCode]: "vscode",
  [IdeId.Cursor]: "cursor",
  [IdeId.WebStorm]: "jetbrains",
  [IdeId.IntelliJ]: "jetbrains",
  [IdeId.PyCharm]: "jetbrains",
  [IdeId.GoLand]: "jetbrains",
  [IdeId.RustRover]: "jetbrains",
  [IdeId.Rider]: "jetbrains",
  [IdeId.CLion]: "jetbrains",
  [IdeId.PhpStorm]: "jetbrains",
  [IdeId.RubyMine]: "jetbrains",
  [IdeId.AndroidStudio]: "androidstudio",
  [IdeId.Zed]: "zed",
  [IdeId.Sublime]: "sublime",
  [IdeId.VisualStudio]: "visualstudio"
};

const FALLBACK_ICON: IconName = "code";
const ADDED_PREFIX = "added-";

// A user-added editor carries an `added-<exe-basename>` id (see ide.rs), whose
// basename can differ from the launcher id — `code`→VS Code, `subl`→Sublime,
// `studio`→Android Studio, and the console editors to the terminal mark.
const ALIAS_ICONS: Record<string, IconName> = {
  code: "vscode",
  subl: "sublime",
  studio: "androidstudio",
  devenv: "visualstudio",
  nvim: "terminal",
  hx: "terminal",
  vim: "terminal"
};

/** A detected editor's brand mark, else the generic code glyph. */
export function ideIcon(id: string): IconName {
  const direct = IDE_ICONS[id];
  if (direct) {
    return direct;
  }

  if (!id.startsWith(ADDED_PREFIX)) {
    return FALLBACK_ICON;
  }

  // Basenames carry version/bit suffixes (`webstorm64`) and often contain the
  // launcher id (`sublime_text`), so match a known id by inclusion first, then
  // fall back to the basename aliases above.
  const basename = id.slice(ADDED_PREFIX.length).toLowerCase();
  const known = Object.values(IdeId).find(value => basename.includes(value));
  if (known) {
    return IDE_ICONS[known];
  }

  const alias = Object.keys(ALIAS_ICONS).find(name => basename.includes(name));
  return alias ? ALIAS_ICONS[alias] : FALLBACK_ICON;
}
