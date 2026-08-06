// Color detection + swatch resolution for the code / config / diff viewers.
// One authoritative home (DRY) for turning a color token — hex, rgb()/rgba(),
// hsl()/hsla(), or var(--x) — into a concrete swatch color. `var(--x)` is traced
// through the file's OWN token definitions first, then the running app's computed
// styles, so the swatch shows the real, accurate color.

const VAR_REF = /^var\(\s*(--[\w-]+)\s*\)$/;
const MAX_TRACE_DEPTH = 8;
export const OPAQUE_ALPHA = 1;
const COMPONENTS_BEFORE_ALPHA = 3;

/** The alpha of a computed color, so a caller can tell an opaque surface from a
 *  see-through one. `rgb(r, g, b)` / `color(srgb r g b)` carry no alpha and are
 *  opaque, `rgba(…, a)` / `color(srgb … / a)` carry it fourth, and `transparent`
 *  (or any value without numeric components) is fully clear. */
export function colorAlpha(color: string): number {
  const components = color.match(/[\d.]+/g);
  if (!components) {
    return 0;
  }

  if (components.length <= COMPONENTS_BEFORE_ALPHA) {
    return OPAQUE_ALPHA;
  }

  return parseFloat(components[COMPONENTS_BEFORE_ALPHA]);
}

/** Is `value` something the engine accepts as a color (so the swatch is real,
 *  never a broken/empty box)? Doubles as the trust-boundary check on file text. */
function isColor(value: string): boolean {
  return typeof CSS !== "undefined" && CSS.supports("color", value);
}

/** Parse a `--name: value;` token map out of stylesheet-ish text, so a file that
 *  defines its own tokens (e.g. a theme.css) traces against itself first. */
export function collectVars(text: string): Map<string, string> {
  const vars = new Map<string, string>();
  for (const match of text.matchAll(/(--[\w-]+)\s*:\s*([^;{}]+);/g)) {
    vars.set(match[1].trim(), match[2].trim());
  }
  return vars;
}

/** Resolve a color token to a concrete CSS color for a swatch, or `null` when it
 *  isn't a real color. Traces `var(--x)` through `vars` then the app's computed
 *  styles, following nested vars up to a small depth. */
export function resolveColor(
  token: string,
  vars?: Map<string, string>,
  depth = 0
): string | null {
  if (depth > MAX_TRACE_DEPTH) {
    return null;
  }

  const trimmed = token.trim();
  const reference = VAR_REF.exec(trimmed);
  if (!reference) {
    return isColor(trimmed) ? trimmed : null;
  }

  const name = reference[1];
  let value = vars?.get(name)?.trim() ?? "";
  if (!value && typeof document !== "undefined") {
    value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  if (!value) {
    return null;
  }

  // The resolved value may itself be another var reference — follow it.
  if (value.startsWith("var(")) {
    return resolveColor(value, vars, depth + 1);
  }

  return isColor(value) ? value : null;
}
