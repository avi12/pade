// Color detection + swatch resolution for the code / config / diff viewers.
// One authoritative home (DRY) for turning a color token - hex, rgb()/rgba(),
// hsl()/hsla(), or var(--x) - into a concrete swatch color. `var(--x)` is traced
// through the file's OWN token definitions first, then the running app's computed
// styles, so the swatch shows the real, accurate color.

const VAR_REFERENCE = /^var\(\s*(--[\w-]+)\s*\)$/;
const VAR_PREFIX = "var(";
const TOKEN_DEFINITION = /(--[\w-]+)\s*:\s*([^;{}]+);/g;
const NUMERIC_COMPONENT = /[\d.]+/g;
const MAX_TRACE_DEPTH = 8;
export const OPAQUE_ALPHA = 1;
const TRANSPARENT_ALPHA = 0;
const COMPONENTS_BEFORE_ALPHA = 3;

/** The alpha of a computed color, so a caller can tell an opaque surface from a
 *  see-through one. `rgb(r, g, b)` / `color(srgb r g b)` carry no alpha and are
 *  opaque, `rgba(…, a)` / `color(srgb … / a)` carry it fourth, and `transparent`
 *  (or any value without numeric components) is fully clear. */
export function colorAlpha(color: string) {
  const components = color.match(NUMERIC_COMPONENT);
  if (!components) {
    return TRANSPARENT_ALPHA;
  }

  if (components.length <= COMPONENTS_BEFORE_ALPHA) {
    return OPAQUE_ALPHA;
  }

  return parseFloat(components[COMPONENTS_BEFORE_ALPHA]);
}

/** Is `value` something the engine accepts as a color (so the swatch is real,
 *  never a broken/empty box)? Doubles as the trust-boundary check on file text. */
function isColor(value: string) {
  return typeof CSS !== "undefined" && CSS.supports("color", value);
}

/** Parse a `--name: value;` token map out of stylesheet-ish text, so a file that
 *  defines its own tokens (e.g. a theme.css) traces against itself first. */
export function collectVars(text: string) {
  const variables = new Map<string, string>();
  for (const match of text.matchAll(TOKEN_DEFINITION)) {
    variables.set(match[1].trim(), match[2].trim());
  }
  return variables;
}

/** Resolve a color token to a concrete CSS color for a swatch, or `null` when it
 *  isn't a real color. Traces `var(--x)` through `variables` then the app's
 *  computed styles, following nested vars up to a small depth. */
export function resolveColor(
  token: string,
  variables?: Map<string, string>,
  depth = 0
) {
  if (depth > MAX_TRACE_DEPTH) {
    return null;
  }

  const trimmed = token.trim();
  const reference = VAR_REFERENCE.exec(trimmed);
  if (!reference) {
    return isColor(trimmed) ? trimmed : null;
  }

  const name = reference[1];
  let value = variables?.get(name)?.trim() ?? "";
  const isDocumentAvailable = typeof document !== "undefined";
  if (!value && isDocumentAvailable) {
    value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  if (!value) {
    return null;
  }

  // The resolved value may itself be another var reference - follow it.
  if (value.startsWith(VAR_PREFIX)) {
    return resolveColor(value, variables, depth + 1);
  }

  return isColor(value) ? value : null;
}

/** sRGB's transfer curve, straight from WCAG 2.x: a linear toe (divided by its
 *  slope) up to the toe limit, then an offset-and-scaled power segment. */
const GAMMA_TOE_LIMIT = 0.03928;
const GAMMA_TOE_SLOPE = 12.92;
const GAMMA_OFFSET = 0.055;
const GAMMA_SCALE = 1.055;
const GAMMA_EXPONENT = 2.4;

/** Channel weights for relative luminance: the eye's sensitivity to red, green
 *  and blue, which is why green dominates and blue barely counts. */
const RED_WEIGHT = 0.2126;
const GREEN_WEIGHT = 0.7152;
const BLUE_WEIGHT = 0.0722;

const HEX_COLOR = /^#([\da-f]{3}|[\da-f]{6})$/i;
const ANY_CHARACTER = /./g;
const SHORT_HEX_LENGTH = 3;
const HEX_DIGITS_PER_CHANNEL = 2;
const HEX_RADIX = 16;
const CHANNEL_MAX = 255;

/** One sRGB channel, 0–255, linearised for the luminance sum. */
function linearChannel(byte: number) {
  const value = byte / CHANNEL_MAX;
  return value <= GAMMA_TOE_LIMIT
    ? value / GAMMA_TOE_SLOPE
    : ((value + GAMMA_OFFSET) / GAMMA_SCALE) ** GAMMA_EXPONENT;
}

/** WCAG relative luminance (0 = black, 1 = white) of a `#rgb`/`#rrggbb` colour,
 *  or `null` when the value is not one. Deliberately hex-only: the callers are
 *  Windows Terminal schemes, whose every colour field is plain hex, and taking
 *  the narrow shape keeps this honest about what it can actually answer. */
export function hexLuminance(hex: string) {
  const match = HEX_COLOR.exec(hex.trim());
  if (!match) {
    return null;
  }

  const digits = match[1];
  const expanded =
    digits.length === SHORT_HEX_LENGTH ? digits.replaceAll(ANY_CHARACTER, digit => digit + digit) : digits;

  function channel(iChannel: number) {
    const start = iChannel * HEX_DIGITS_PER_CHANNEL;
    return linearChannel(parseInt(expanded.slice(start, start + HEX_DIGITS_PER_CHANNEL), HEX_RADIX));
  }

  return RED_WEIGHT * channel(0) + GREEN_WEIGHT * channel(1) + BLUE_WEIGHT * channel(2);
}

/** Half luminance - the split between a colour that paints a dark surface and
 *  one that paints a light one. The only question a light/dark match asks. */
const DARK_SURFACE_LIMIT = 0.5;

/** Whether a colour reads as a dark surface. An unparseable value is not called
 *  dark: the honest answer to "is this dark?" for something we cannot measure is
 *  "no claim", and a false warning is worse than a missing one. */
export function isDarkSurface(hex: string) {
  const luminance = hexLuminance(hex);
  return luminance !== null && luminance < DARK_SURFACE_LIMIT;
}
