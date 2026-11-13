import { z } from "zod";

import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import type { PathData } from "@/lib/ipc/domains/path-data";
import { DesignTool, EditorKind, Ide, LanguageShare } from "@/lib/types/editor";
import { Settings } from "@/lib/types/settings";

export const EDITOR_COMMANDS = {
  IDE_DETECT: "ide_detect",
  IDE_SUGGEST: "ide_suggest",
  IDE_CHOOSE_EDITOR: "ide_choose_editor",
  IDE_KINDS: "ide_kinds",
  IDE_KIND_OPTIONS: "ide_kind_options",
  IDE_PROJECT_KINDS: "ide_project_kinds",
  IDE_PROJECT_LANGUAGES: "ide_project_languages",
  IDE_ADD_EDITOR: "ide_add_editor",
  IDE_REMOVE_EDITOR: "ide_remove_editor",
  IDE_OPEN: "ide_open",
  IDE_OPEN_FILE: "ide_open_file",
  OPEN_IN_EXPLORER: "open_in_explorer",
  REVEAL_IN_EXPLORER: "reveal_in_explorer",
  OPEN_IN_TERMINAL: "open_in_terminal",
  OPEN_URL: "open_url",
  DESIGN_TOOLS: "design_tools",
  DESIGN_OPEN: "design_open"
} as const;

export interface EditorCommandProtocol {
  [EDITOR_COMMANDS.IDE_DETECT]: () => Ide[];
  /** Installed IDEs ordered for the given project: explicit per-project choice
   *  → rule → fallback → language-coverage ranking. `ides[0]` is *the* editor
   *  on every surface. */
  [EDITOR_COMMANDS.IDE_SUGGEST]: (data: { cwd: string }) => Ide[];
  /** Persist an explicit editor pick for the project at `cwd`. */
  [EDITOR_COMMANDS.IDE_CHOOSE_EDITOR]: (data: { cwd: string; id: string }) => Settings;
  /** The project kinds the rules engine shows, in render/priority order. */
  [EDITOR_COMMANDS.IDE_KINDS]: () => EditorKind[];
  /** Editor ids suited to each project kind (kind → ordered, installed-only). */
  [EDITOR_COMMANDS.IDE_KIND_OPTIONS]: () => Record<string, string[]>;
  /** Primary detected kind per project path; unrecognised paths are omitted. */
  [EDITOR_COMMANDS.IDE_PROJECT_KINDS]: (data: { paths: string[] }) => Record<string, string>;
  /** Each project's languages by share of its source, biggest first;
   *  paths with no countable source are omitted. */
  [EDITOR_COMMANDS.IDE_PROJECT_LANGUAGES]: (data: { paths: string[] }) => Record<string, LanguageShare[]>;
  /** Add an editor by its executable path; rejects with the message when it
   *  isn't a supported editor. */
  [EDITOR_COMMANDS.IDE_ADD_EDITOR]: (data: PathData) => Settings;
  [EDITOR_COMMANDS.IDE_REMOVE_EDITOR]: (data: { id: string }) => Settings;
  /** `line` is 1-based, meaningful only when `path` is a file. */
  [EDITOR_COMMANDS.IDE_OPEN]: (data: { command: string; path?: string; line?: number }) => void;
  /** Open a file inside its `project` workspace window, jumping to `line`. */
  [EDITOR_COMMANDS.IDE_OPEN_FILE]: (data: { command: string; project: string; file: string; line?: number }) => void;
  [EDITOR_COMMANDS.OPEN_IN_EXPLORER]: (data: PathData) => void;
  /** Show a file selected in its folder - never opens or runs the file itself. */
  [EDITOR_COMMANDS.REVEAL_IN_EXPLORER]: (data: PathData) => void;
  [EDITOR_COMMANDS.OPEN_IN_TERMINAL]: (data: PathData) => void;
  /** Open an http(s) URL in the system's default browser. */
  [EDITOR_COMMANDS.OPEN_URL]: (data: { url: string }) => void;
  /** AI design/UI-generation tools, ranked for the active agent. */
  [EDITOR_COMMANDS.DESIGN_TOOLS]: (data: { agent: string }) => DesignTool[];
  /** Open the picked tool in the companion PADE window (iframes are blocked). */
  [EDITOR_COMMANDS.DESIGN_OPEN]: (data: { id: string }) => void;
}

export const EDITOR_RESPONSE_SCHEMAS = {
  [EDITOR_COMMANDS.IDE_DETECT]: z.array(Ide),
  [EDITOR_COMMANDS.IDE_SUGGEST]: z.array(Ide),
  [EDITOR_COMMANDS.IDE_CHOOSE_EDITOR]: Settings,
  [EDITOR_COMMANDS.IDE_KINDS]: z.array(EditorKind),
  [EDITOR_COMMANDS.IDE_KIND_OPTIONS]: z.record(z.string(), z.array(z.string())),
  [EDITOR_COMMANDS.IDE_PROJECT_KINDS]: z.record(z.string(), z.string()),
  [EDITOR_COMMANDS.IDE_PROJECT_LANGUAGES]: z.record(z.string(), z.array(LanguageShare)),
  [EDITOR_COMMANDS.IDE_ADD_EDITOR]: Settings,
  [EDITOR_COMMANDS.IDE_REMOVE_EDITOR]: Settings,
  [EDITOR_COMMANDS.IDE_OPEN]: NO_REPLY,
  [EDITOR_COMMANDS.IDE_OPEN_FILE]: NO_REPLY,
  [EDITOR_COMMANDS.OPEN_IN_EXPLORER]: NO_REPLY,
  [EDITOR_COMMANDS.REVEAL_IN_EXPLORER]: NO_REPLY,
  [EDITOR_COMMANDS.OPEN_IN_TERMINAL]: NO_REPLY,
  [EDITOR_COMMANDS.OPEN_URL]: NO_REPLY,
  [EDITOR_COMMANDS.DESIGN_TOOLS]: z.array(DesignTool),
  [EDITOR_COMMANDS.DESIGN_OPEN]: NO_REPLY
} satisfies { [Key in keyof EditorCommandProtocol]: z.ZodType<ReturnType<EditorCommandProtocol[Key]>> };
