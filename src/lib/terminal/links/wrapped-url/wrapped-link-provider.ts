// Registers the wrapped-URL link provider with xterm: overrides the OSC-8 and addon
// URL links, and paints the whole-URL hover underline.

import type { ILink, ILinkProvider } from "@xterm/xterm";
import { z } from "zod";
import { URL_PATTERN, isUrl } from "@/lib/terminal/links/wrapped-url/url-pattern";
import { cellText } from "@/lib/terminal/links/wrapped-url/row-content";
import type { LinkBuffer, LinkTerminal } from "@/lib/terminal/links/wrapped-url/row-content";
import { computeLinks } from "@/lib/terminal/links/wrapped-url/wrapped-lines";
import type { RowSpan, WrappedLink } from "@/lib/terminal/links/wrapped-url/wrapped-lines";

// The slice of xterm's `Terminal` this registration needs: what `computeLinks`
// reads, plus `registerLinkProvider` and the DOM/geometry the hover overlay uses
// (`element` to reach the screen, `rows` + `viewportY` to map a buffer row to a
// viewport row). Narrow enough that a test builds a plain mock, while the real
// `Terminal` satisfies it structurally.
interface RegisterableTerminal {
  cols: number;
  rows: number;
  element?: HTMLElement;
  buffer: {
    active: LinkBuffer & {
      viewportY: number;
    };
  };
  registerLinkProvider(provider: ILinkProvider): unknown;
}

const XTERM_SCREEN_SELECTOR = ".xterm-screen";

const UNDERLINE_CLASS = "terminal-link-underline";

// xterm's screen element and its cell geometry, or null before the terminal has
// mounted. The screen is exactly `cols × rows` cells with no padding, so a cell's
// pixel size is just the screen size over the grid - no need to reach into
// xterm's private render service.
function screenGeometry(terminal: RegisterableTerminal) {
  const elScreen = terminal.element?.querySelector<HTMLElement>(XTERM_SCREEN_SELECTOR);
  if (!elScreen) {
    return null;
  }

  return {
    elScreen,
    cellWidth: elScreen.clientWidth / terminal.cols,
    cellHeight: elScreen.clientHeight / terminal.rows
  };
}

// Paint a tight underline under every row-span of a hovered URL, appended into
// xterm's screen so it scrolls with the text; returns the elements to remove on
// leave. A span above or below the viewport maps to an off-screen row and is
// simply positioned there (xterm clips its screen), so nothing extra is needed.
function paintUnderline({ terminal, spans }: {
  terminal: RegisterableTerminal;
  spans: RowSpan[];
}) {
  const geometry = screenGeometry(terminal);
  if (!geometry) {
    return [];
  }

  const viewportRow = terminal.buffer.active.viewportY;
  const elUnderlines: HTMLElement[] = [];
  for (const span of spans) {
    const elUnderline = document.createElement("div");
    elUnderline.className = UNDERLINE_CLASS;
    elUnderline.style.left = `${span.startColumn * geometry.cellWidth}px`;
    elUnderline.style.top = `${(span.row - viewportRow) * geometry.cellHeight}px`;
    elUnderline.style.width = `${(span.endColumn - span.startColumn + 1) * geometry.cellWidth}px`;
    elUnderline.style.height = `${geometry.cellHeight}px`;
    geometry.elScreen.appendChild(elUnderline);
    elUnderlines.push(elUnderline);
  }

  return elUnderlines;
}

// xterm's ordered list of link providers - its own OSC-8 provider is registered
// first at terminal construction, and this list is where its precedence lives.
interface InternalLinkProviderService {
  linkProviders: ILinkProvider[];
}

// The shape xterm's link-provider service must have. Kept minimal - we only
// reorder the providers, so the elements stay `z.unknown()` and the guard's
// return type brands them `ILinkProvider`.
const LinkProviderServiceSchema = z.object({
  linkProviders: z.array(z.unknown())
});

// A Zod-backed type guard - the idiomatic v4 way to narrow an `unknown` (v4
// dropped `.refine` type-predicate narrowing). `safeParse` validates the shape
// with no cast and no `z.custom`, and returning the ORIGINAL `value` keeps
// xterm's LIVE object, so reordering its `linkProviders` reorders xterm itself.
function isLinkProviderService(value: unknown): value is InternalLinkProviderService {
  return LinkProviderServiceSchema.safeParse(value).success;
}

// `_core` and the service are private and unexported, so instead of a type
// assertion (which blindly trusts the shape) the path is validated through Zod
// at this trust boundary. The service hops stay `z.unknown()` so their values
// pass through by REFERENCE - a cloning `z.object`/`z.array` on the array would
// hand back a dead copy - then the guard narrows the live service. A renamed
// field just yields null, leaving the provider where it is (still correct, only
// lower-priority). xterm names it `_linkProviderService`; tolerate an unprefixed
// alias across versions.
const TerminalInternals = z.object({
  _core: z.object({
    _linkProviderService: z.unknown().optional(),
    linkProviderService: z.unknown().optional()
  })
});

function linkProviderService(terminal: RegisterableTerminal) {
  const parsed = TerminalInternals.safeParse(terminal);
  if (!parsed.success) {
    return null;
  }

  const { _core } = parsed.data;
  const service = _core._linkProviderService ?? _core.linkProviderService;
  return isLinkProviderService(service) ? service : null;
}

// Whether this built-in OSC-8 link's VISIBLE anchor is itself a URL - the
// tell-tale of an agent that emitted a plain URL as an OSC-8 hyperlink (Claude,
// OpenCode), as opposed to a labelled link whose anchor is prose ("Security
// guide"). The anchor's glyphs are read straight off its cells and matched with
// the same `URL_PATTERN` + `isUrl` our provider uses, so the two never disagree on
// what counts as a URL. A multi-row anchor is judged by its starting row alone -
// the scheme and host live there, so the first row's glyphs already match. Our
// provider trims the URL's trailing sentence punctuation while the OSC-8 anchor
// keeps it; deferring every URL-shaped anchor to our provider is what stops
// xterm's resolution from serving that trailing `.`/`,`/`)` off the anchor behind
// us.
function anchorIsUrl({ terminal, link }: {
  terminal: LinkTerminal;
  link: ILink;
}) {
  const line = terminal.buffer.active.getLine(link.range.start.y - 1);
  if (!line) {
    return false;
  }

  const firstColumn = link.range.start.x - 1;
  const isOnOneRow = link.range.end.y === link.range.start.y;
  const lastColumn = isOnOneRow ? link.range.end.x - 1 : terminal.cols - 1;
  let anchor = "";
  for (let column = firstColumn; column <= lastColumn; column += 1) {
    anchor += cellText(line.getCell(column));
  }

  const match = anchor.match(URL_PATTERN);
  return match !== null && isUrl(match[0]);
}

// Wrap a built-in link provider so any link our URL provider already owns is
// dropped, deferring every URL-shaped OSC-8 hyperlink to the punctuation-trimmed,
// wrap-stitched range our provider computes - while labelled and `file:` OSC-8
// links, which our provider never matches, pass straight through to activate as
// before.
function deferUrlsFrom({ terminal, provider }: {
  terminal: LinkTerminal;
  provider: ILinkProvider;
}) {
  return {
    provideLinks(bufferLineNumber, callback) {
      provider.provideLinks(bufferLineNumber, links => {
        if (!links) {
          callback(links);
          return;
        }

        callback(
          links.filter(link => !anchorIsUrl({
            terminal,
            link
          }))
        );
      });
    }
  } satisfies ILinkProvider;
}

// Make our URL provider the authority for every URL-shaped link. xterm registers
// its own OSC-8 provider first at construction, and its resolution serves the
// FIRST provider (by list order) that has a link under the cursor - so an agent
// that emits a URL as an OSC-8 hyperlink (Claude, OpenCode) would otherwise be
// served by that provider, whose range spans the whole agent-tagged anchor,
// including a trailing sentence `.`/`,`/`)` our regex trims. This wraps every
// built-in provider to drop the links our provider already owns, then moves ours
// to the front, so its regex-parsed, wrap-stitched, punctuation-trimmed range
// wins outright and no trimmed punctuation survives on the anchor behind it. A
// labelled OSC-8 link (whose visible text isn't a URL) matches nothing here and
// still activates through the built-in provider.
function overrideOscUrlLinks({ terminal, provider }: {
  terminal: RegisterableTerminal;
  provider: ILinkProvider;
}) {
  const service = linkProviderService(terminal);
  if (!service) {
    return;
  }

  const { linkProviders } = service;
  for (const [i, builtIn] of linkProviders.entries()) {
    if (builtIn !== provider) {
      linkProviders[i] = deferUrlsFrom({
        terminal,
        provider: builtIn
      });
    }
  }

  const iProvider = linkProviders.indexOf(provider);
  if (iProvider > 0) {
    linkProviders.splice(iProvider, 1);
    linkProviders.unshift(provider);
  }
}

// Register a link provider that makes http(s) URLs clickable, rejoining soft-,
// hard-, and self-wrapped rows so a wrapped URL activates and underlines in full.
// `openUrl` receives the whole URL - route it to the system browser. It overrides
// xterm's built-in OSC-8 provider for URL-shaped links so an agent's OSC-8-tagged
// URL gets the same full-URL, no-trailing-punctuation range as a plain-text one;
// labelled OSC-8 links still reach that provider.
export function registerWrappedLinkProvider({ terminal, openUrl }: {
  terminal: RegisterableTerminal;
  openUrl: (uri: string) => void;
}) {
  const provider: ILinkProvider = {
    provideLinks(bufferLineNumber, callback) {
      const links = computeLinks({
        terminal,
        bufferLineNumber,
        openUrl
      });
      callback(
        links.map(link => withHoverUnderline({
          terminal,
          link
        }))
      );
    }
  };
  terminal.registerLinkProvider(provider);
  overrideOscUrlLinks({
    terminal,
    provider
  });
}

// Wrap a computed link so hovering paints a tight underline over the WHOLE URL -
// every row-span, not just the hovered one - and leaving (or discarding the link)
// removes it. The built-in underline is off, so these elements own the visual.
function withHoverUnderline({ terminal, link }: {
  terminal: RegisterableTerminal;
  link: WrappedLink;
}) {
  let elUnderlines: HTMLElement[] = [];
  function clear() {
    for (const elUnderline of elUnderlines) {
      elUnderline.remove();
    }

    elUnderlines = [];
  }

  return {
    text: link.text,
    range: link.range,
    decorations: link.decorations,
    activate: link.activate,
    hover() {
      clear();
      elUnderlines = paintUnderline({
        terminal,
        spans: link.spans
      });
    },
    leave: clear,
    dispose: clear
  };
}
