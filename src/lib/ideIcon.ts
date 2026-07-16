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

// Brand mark per detected editor. Each JetBrains product carries its own
// official icon (the full-colour gradient square, vendored from
// github.com/JetBrains/logos) — the icon name matches the editor id.
const IDE_ICONS: Record<string, IconName> = {
  [IdeId.VsCode]: "vscode",
  [IdeId.Cursor]: "cursor",
  [IdeId.WebStorm]: "webstorm",
  [IdeId.IntelliJ]: "idea",
  [IdeId.PyCharm]: "pycharm",
  [IdeId.GoLand]: "goland",
  [IdeId.RustRover]: "rustrover",
  [IdeId.Rider]: "rider",
  [IdeId.CLion]: "clion",
  [IdeId.PhpStorm]: "phpstorm",
  [IdeId.RubyMine]: "rubymine",
  [IdeId.AndroidStudio]: "androidstudio",
  [IdeId.Zed]: "zed",
  [IdeId.Sublime]: "sublime",
  [IdeId.VisualStudio]: "visualstudio"
};

const FALLBACK_ICON: IconName = "code";
const ADDED_PREFIX = "added-";

// A user-added editor carries an `added-<exe-basename>` id (see ide.rs), whose
// basename can differ from the launcher id — these aliases bridge the gap
// (`code` → VS Code, `subl` → Sublime, `studio` → Android Studio).
const ALIAS_IDS: Record<string, IdeId> = {
  code: IdeId.VsCode,
  subl: IdeId.Sublime,
  studio: IdeId.AndroidStudio,
  devenv: IdeId.VisualStudio
};

// Console editors have no brand mark or tint — they get the terminal glyph.
const CONSOLE_EDITOR_NAMES = ["nvim", "hx", "vim"] as const;

/** The canonical editor id behind a detected or user-added id — `added-<exe>`
 *  basenames carry version/bit suffixes (`webstorm64`) and often contain the
 *  launcher id (`sublime_text`), so known ids match by inclusion. Null for a
 *  console editor or anything unrecognised. */
export function canonicalIdeId(id: string): IdeId | null {
  const direct = Object.values(IdeId).find(value => value === id);
  if (direct) {
    return direct;
  }

  if (!id.startsWith(ADDED_PREFIX)) {
    return null;
  }

  const basename = id.slice(ADDED_PREFIX.length).toLowerCase();
  const known = Object.values(IdeId).find(value => basename.includes(value));
  if (known) {
    return known;
  }

  const alias = Object.keys(ALIAS_IDS).find(name => basename.includes(name));
  return alias ? ALIAS_IDS[alias] : null;
}

/** A detected editor's brand mark, else the generic code glyph. */
export function ideIcon(id: string): IconName {
  const canonical = canonicalIdeId(id);
  if (canonical) {
    return IDE_ICONS[canonical];
  }

  const basename = id.startsWith(ADDED_PREFIX) ? id.slice(ADDED_PREFIX.length).toLowerCase() : "";
  const isConsoleEditor = CONSOLE_EDITOR_NAMES.some(name => basename.includes(name));
  return isConsoleEditor ? "terminal" : FALLBACK_ICON;
}

/** The editor's brand-tint key (theme.css `[data-brand]`) — the canonical id.
 *  A full-colour mark (the JetBrains products) simply ignores the tint; its
 *  fills are baked in. Undefined when there's no brand to tint (a console or
 *  unknown editor), which omits the attribute and leaves the text colour. */
export function ideBrand(id: string): IdeId | undefined {
  return canonicalIdeId(id) ?? undefined;
}
