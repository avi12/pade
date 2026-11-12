import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import type { PathData } from "@/lib/ipc/domains/path-data";
import { DesignTool, EditorKind, Ide, LanguageShare } from "@/lib/types/editor";
import { Settings } from "@/lib/types/settings";
import { z } from "zod";

export const EDITOR_COMMANDS = {
  ideDetect: "ide_detect",
  ideSuggest: "ide_suggest",
  ideChooseEditor: "ide_choose_editor",
  ideKinds: "ide_kinds",
  ideKindOptions: "ide_kind_options",
  ideProjectKinds: "ide_project_kinds",
  ideProjectLanguages: "ide_project_languages",
  ideAddEditor: "ide_add_editor",
  ideRemoveEditor: "ide_remove_editor",
  ideOpen: "ide_open",
  ideOpenFile: "ide_open_file",
  openInExplorer: "open_in_explorer",
  revealInExplorer: "reveal_in_explorer",
  openInTerminal: "open_in_terminal",
  openUrl: "open_url",
  designTools: "design_tools",
  designOpen: "design_open"
} as const;

export interface EditorCommandProtocol {
  [EDITOR_COMMANDS.ideDetect]: () => Ide[];
  /** Installed IDEs ordered for the given project: explicit per-project choice
   *  → rule → fallback → language-coverage ranking. `ides[0]` is *the* editor
   *  on every surface. */
  [EDITOR_COMMANDS.ideSuggest]: (data: { cwd: string }) => Ide[];
  /** Persist an explicit editor pick for the project at `cwd`. */
  [EDITOR_COMMANDS.ideChooseEditor]: (data: {
    cwd: string;
    id: string;
  }) => Settings;
  /** The project kinds the rules engine shows, in render/priority order. */
  [EDITOR_COMMANDS.ideKinds]: () => EditorKind[];
  /** Editor ids suited to each project kind (kind → ordered, installed-only). */
  [EDITOR_COMMANDS.ideKindOptions]: () => Record<string, string[]>;
  /** Primary detected kind per project path; unrecognised paths are omitted. */
  [EDITOR_COMMANDS.ideProjectKinds]: (data: { paths: string[] }) => Record<string, string>;
  /** Each project's languages by share of its source, biggest first;
   *  paths with no countable source are omitted. */
  [EDITOR_COMMANDS.ideProjectLanguages]: (data: { paths: string[] }) => Record<string, LanguageShare[]>;
  /** Add an editor by its executable path; rejects with the message when it
   *  isn't a supported editor. */
  [EDITOR_COMMANDS.ideAddEditor]: (data: PathData) => Settings;
  [EDITOR_COMMANDS.ideRemoveEditor]: (data: { id: string }) => Settings;
  /** `line` is 1-based, meaningful only when `path` is a file. */
  [EDITOR_COMMANDS.ideOpen]: (data: {
    command: string;
    path?: string;
    line?: number;
  }) => void;
  /** Open a file inside its `project` workspace window, jumping to `line`. */
  [EDITOR_COMMANDS.ideOpenFile]: (data: {
    command: string;
    project: string;
    file: string;
    line?: number;
  }) => void;
  [EDITOR_COMMANDS.openInExplorer]: (data: PathData) => void;
  /** Show a file selected in its folder - never opens or runs the file itself. */
  [EDITOR_COMMANDS.revealInExplorer]: (data: PathData) => void;
  [EDITOR_COMMANDS.openInTerminal]: (data: PathData) => void;
  /** Open an http(s) URL in the system's default browser. */
  [EDITOR_COMMANDS.openUrl]: (data: { url: string }) => void;
  /** AI design/UI-generation tools, ranked for the active agent. */
  [EDITOR_COMMANDS.designTools]: (data: { agent: string }) => DesignTool[];
  /** Open the picked tool in the companion PADE window (iframes are blocked). */
  [EDITOR_COMMANDS.designOpen]: (data: { id: string }) => void;
}

export const EDITOR_RESPONSE_SCHEMAS = {
  [EDITOR_COMMANDS.ideDetect]: z.array(Ide),
  [EDITOR_COMMANDS.ideSuggest]: z.array(Ide),
  [EDITOR_COMMANDS.ideChooseEditor]: Settings,
  [EDITOR_COMMANDS.ideKinds]: z.array(EditorKind),
  [EDITOR_COMMANDS.ideKindOptions]: z.record(z.string(), z.array(z.string())),
  [EDITOR_COMMANDS.ideProjectKinds]: z.record(z.string(), z.string()),
  [EDITOR_COMMANDS.ideProjectLanguages]: z.record(z.string(), z.array(LanguageShare)),
  [EDITOR_COMMANDS.ideAddEditor]: Settings,
  [EDITOR_COMMANDS.ideRemoveEditor]: Settings,
  [EDITOR_COMMANDS.ideOpen]: NO_REPLY,
  [EDITOR_COMMANDS.ideOpenFile]: NO_REPLY,
  [EDITOR_COMMANDS.openInExplorer]: NO_REPLY,
  [EDITOR_COMMANDS.revealInExplorer]: NO_REPLY,
  [EDITOR_COMMANDS.openInTerminal]: NO_REPLY,
  [EDITOR_COMMANDS.openUrl]: NO_REPLY,
  [EDITOR_COMMANDS.designTools]: z.array(DesignTool),
  [EDITOR_COMMANDS.designOpen]: NO_REPLY
} satisfies { [Key in keyof EditorCommandProtocol]: z.ZodType<ReturnType<EditorCommandProtocol[Key]>> };
