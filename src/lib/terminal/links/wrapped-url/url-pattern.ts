// The URL pattern and URL-validity check ported from @xterm/addon-web-links (MIT, the
// xterm.js authors) because its internals aren't exported. Keep them in sync if that
// addon is upgraded.

// Matches an http(s) URL. Copied verbatim from @xterm/addon-web-links so both
// paths detect exactly the same links. Its trailing character class excludes
// sentence punctuation, so a URL ending a sentence doesn't swallow the period.
export const URL_PATTERN = /(https?|HTTPS?):[/]{2}[^\s"'!*(){}|\\^<>`]*[^\s"':,.!?{}|\\^~[\]`()<>]/;

/** The same pattern for walking every URL in a text with `matchAll`, which
 *  iterates on its own copy so no `lastIndex` leaks between calls. */
export const URL_PATTERN_GLOBAL = new RegExp(URL_PATTERN.source, "g");

/** The scheme+`//` that anchors a URL. */
const URL_SCHEME = /(https?):[/]{2}/i;

const WHITESPACE = /\s/;

// The `protocol//[user[:pass]@]host` prefix a real URL must start with - built
// with guard clauses so the credential variants read top-to-bottom.
function authorityOf(url: URL) {
  const base = `${url.protocol}//`;
  if (url.username && url.password) {
    return `${base}${url.username}:${url.password}@${url.host}`;
  }

  if (url.username) {
    return `${base}${url.username}@${url.host}`;
  }

  return `${base}${url.host}`;
}

// The matched text really forms a URL (protocol//[user[:pass]@]host…). Ported
// from the addon: it rejects near-misses the permissive pattern would otherwise
// accept (a bare `https://` with no host).
export function isUrl(candidate: string) {
  try {
    const authority = authorityOf(new URL(candidate));
    return candidate.toLocaleLowerCase().startsWith(authority.toLocaleLowerCase());
  } catch {
    return false;
  }
}

// A window that already contains a URL scheme holds the URL's start; one that
// doesn't is a bare fragment whose start lies further up.
export function hasScheme(text: string) {
  return URL_SCHEME.test(text);
}

// A whitespace-free token - what every INNER row of a wrapped URL is (URLs never
// contain spaces), and what prose never is. Lets the upward walk cross a URL's
// scheme-less continuation rows to the row that starts it.
export function isBareToken(text: string) {
  return text.length > 0 && !WHITESPACE.test(text);
}

// A clickable URL, plus every row-span it occupies. The `range` is this link's
// own single row (so xterm's hit-testing stays tight - a self-wrapping agent's
// blank padding, outside any span, is never clickable); `spans` is the whole URL
// across all its rows, so hovering any part can underline the entire address.
/** Every valid http(s) URL in `text` with its start offset - the one matcher
 *  both the terminal's link provider and the plain-HTML runner output use, so
 *  they agree on what is a link and where it ends. */
export function findUrls(text: string) {
  const urls: {
    url: string;
    start: number;
  }[] = [];
  for (const match of text.matchAll(URL_PATTERN_GLOBAL)) {
    if (isUrl(match[0])) {
      urls.push({
        url: match[0],
        start: match.index
      });
    }
  }

  return urls;
}
