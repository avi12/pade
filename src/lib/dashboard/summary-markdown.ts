// The inline Markdown an agent summary may carry - **bold**, *emphasis*,
// `code`, [text](url) links and bare web links - split into pieces the
// dashboard renders as real elements. Never HTML: a summary is model output
// read off another window, so it can't inject markup, and a link is only ever
// an http(s) address.

export const InlineKind = {
  Text: "text",
  Strong: "strong",
  Emphasis: "emphasis",
  Code: "code",
  Link: "link"
} as const;
type InlineKind = (typeof InlineKind)[keyof typeof InlineKind];

type InlinePiece = {
  kind: InlineKind;
  text: string;
  /** A link's web address. */
  url?: string;
};

/** One pass over the source, each alternative a named group: a code span first
 *  (its content is never read as anything else), then a link, a bare address,
 *  bold before emphasis. */
const INLINE_PATTERN = new RegExp(
  [
    "`(?<code>[^`]+)`",
    "\\[(?<label>[^\\]]+)\\]\\((?<target>https?://[^\\s)]+)\\)",
    "(?<bare>https?://[^\\s)\\]]+)",
    "\\*\\*(?<strong>[^*]+)\\*\\*",
    "\\*(?<starred>[^*\\s][^*]*)\\*",
    "(?<![\\w])_(?<underscored>[^_\\s][^_]*)_(?![\\w])"
  ].join("|"),
  "g"
);

function pieceOf(groups: Record<string, string | undefined>): InlinePiece {
  const { code, label, target, bare, strong, starred, underscored } = groups;
  if (code !== undefined) {
    return {
      kind: InlineKind.Code,
      text: code
    };
  }

  if (label !== undefined && target !== undefined) {
    return {
      kind: InlineKind.Link,
      text: label,
      url: target
    };
  }

  if (bare !== undefined) {
    return {
      kind: InlineKind.Link,
      text: bare,
      url: bare
    };
  }

  if (strong !== undefined) {
    return {
      kind: InlineKind.Strong,
      text: strong
    };
  }

  return {
    kind: InlineKind.Emphasis,
    text: starred ?? underscored ?? ""
  };
}

/** Split a summary into its plain and formatted pieces, in order. */
export function inlineMarkdown(source: string) {
  const pieces: InlinePiece[] = [];
  let cursor = 0;
  for (const match of source.matchAll(INLINE_PATTERN)) {
    if (match.index > cursor) {
      pieces.push({
        kind: InlineKind.Text,
        text: source.slice(cursor, match.index)
      });
    }

    pieces.push(pieceOf(match.groups ?? {}));
    cursor = match.index + match[0].length;
  }

  if (cursor < source.length) {
    pieces.push({
      kind: InlineKind.Text,
      text: source.slice(cursor)
    });
  }

  return pieces;
}
