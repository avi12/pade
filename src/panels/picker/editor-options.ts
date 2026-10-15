import type { Ide } from "@/lib/types";

/** The detected editors that suit a project kind, in the backend's priority
 *  order. An unknown kind (no entry) falls back to every editor rather than
 *  hiding them all; an id that's no longer detected is skipped. */
export function editorsForKind({ ides, kindOptions, kind }: {
  ides: Ide[];
  kindOptions: Record<string, string[]>;
  kind: string;
}) {
  const ids = kindOptions[kind];
  if (!ids) {
    return ides;
  }

  return ids
    .map(id => ides.find(editor => editor.id === id))
    .filter((editor): editor is Ide => editor !== undefined);
}

/** A stable, valid popover id/anchor for one editor select (a kind or
 *  "fallback"). */
export function editorSelectId(key: string) {
  return `ide-${key.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
}
