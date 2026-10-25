import { Key } from "@/lib/ui/keyboard/keys";
import { nextIndex } from "@/lib/ui/keyboard/roving-menu";
import { baseName } from "@/lib/util/paths";

/** How many existing projects the new-project name field suggests at most. */
export const MAX_PROJECT_SUGGESTIONS = 5;

/** One project the composer can switch to: its path and the name it shows. */
export interface ProjectOption {
  path: string;
  name: string;
}

/** The existing projects a typed new-project name matches, in the order given
 *  (the project menu's recents first, then the scanned rest): a project whose
 *  display name or folder name holds the text, ignoring case. Nothing for a
 *  blank name, and never more than `MAX_PROJECT_SUGGESTIONS`. */
export function matchingProjects({ options, typed }: {
  options: readonly ProjectOption[];
  typed: string;
}) {
  const query = typed.trim().toLowerCase();
  if (query.length === 0) {
    return [];
  }

  const matches: ProjectOption[] = [];
  for (const option of options) {
    const isMatch = option.name.toLowerCase().includes(query) || baseName(option.path).toLowerCase().includes(query);
    if (!isMatch) {
      continue;
    }

    matches.push(option);

    if (matches.length === MAX_PROJECT_SUGGESTIONS) {
      break;
    }
  }

  return matches;
}

/** What a key in the name field does to its suggestions: move the highlight
 *  (or close the list), or pick the highlighted project. */
export const SuggestionStepKind = {
  Move: "move",
  Pick: "pick"
} as const;

/** The suggestions' answer to a key in the name field, or null for a key
 *  they leave to the field: Up/Down highlight the next row (wrapping, and
 *  reopening a dismissed list), Enter picks the highlighted row, Escape
 *  closes an open list. Enter with nothing highlighted is the field's, so it
 *  still starts the new project. */
export function suggestionKeyStep({ key, highlightedIndex, count, isListVisible }: {
  key: string;
  /** The highlighted row, -1 while none. */
  highlightedIndex: number;
  count: number;
  isListVisible: boolean;
}) {
  const highlighted = nextIndex({
    key,
    from: highlightedIndex,
    count
  });
  const isArrow = key === Key.ArrowDown || key === Key.ArrowUp;
  if (isArrow && highlighted !== null) {
    return {
      kind: SuggestionStepKind.Move,
      activeIndex: highlighted,
      isListOpen: true
    };
  }

  const isPickingHighlighted = key === Key.Enter && highlightedIndex >= 0;
  if (isPickingHighlighted) {
    return {
      kind: SuggestionStepKind.Pick,
      index: highlightedIndex
    };
  }

  const isDismissing = key === Key.Escape && isListVisible;
  if (isDismissing) {
    return {
      kind: SuggestionStepKind.Move,
      activeIndex: -1,
      isListOpen: false
    };
  }

  return null;
}
