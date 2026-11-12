import { z } from "zod";

export const Ide = z.object({
  id: z.string(),
  label: z.string(),
  command: z.string(),
  /** A console editor (Neovim, Vim, Helix) PADE opens in a terminal tab rather
   *  than launching as a detached window. */
  terminal: z.boolean().default(false),
  /** Leads `ide_suggest`'s ranking because of an explicit per-project pick -
   *  badged as the user's choice, not the auto-detected best fit. */
  chosen: z.boolean().default(false)
});

export type Ide = z.infer<typeof Ide>;

/** One project kind the editor-rules engine renders a row for. The kind
 *  registry lives in Rust (`ide_kinds`, render/priority order); the frontend
 *  derives its rows from it rather than hardcoding the list. */
export const EditorKind = z.object({
  kind: z.string(),
  label: z.string(),
  signals: z.array(z.string())
});

export type EditorKind = z.infer<typeof EditorKind>;

/** One language's share of a project's source (`ide_project_languages`), for
 *  the switcher rows' GitHub-style language bar. Shares are of the languages
 *  shown, so they fill the bar. */
export const LanguageShare = z.object({
  /** The GitHub Linguist name - the bar's label and colour key. */
  language: z.string(),
  /** Its family, for a project row's icon. */
  kind: z.string(),
  perMille: z.number().int().min(0).max(1000)
});

export type LanguageShare = z.infer<typeof LanguageShare>;

/** An editor the user located by executable path (merged into detection). */
export const AddedEditor = z.object({
  id: z.string(),
  label: z.string(),
  path: z.string()
});

/** An AI design/UI-generation tool, ranked for the active agent. */
export const DesignTool = z.object({
  id: z.string(),
  label: z.string(),
  vendor: z.string(),
  url: z.string(),
  recommended: z.boolean()
});

export type DesignTool = z.infer<typeof DesignTool>;
