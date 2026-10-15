// A model id as a person reads it: `claude-opus-5-5` → "Opus 5.5",
// `claude-haiku-4-5-20251001` → "Haiku 4.5", `anthropic/claude-opus-5` →
// "Opus 5", `openai/gpt-5.6-sol` → "gpt-5.6-sol". Any id that isn't Claude's
// `claude-<family>-<version…>[-<date>]` shape is shown as it was logged, minus
// a provider prefix.

const CLAUDE_PREFIX = "claude-";
const PROVIDER_SEPARATOR = "/";
const ID_PART_SEPARATOR = "-";
const VERSION_SEPARATOR = ".";
const RELEASE_DATE = /^\d{8}$/;
const VERSION_PART = /^\d+$/;
const NOT_FOUND = -1;

/** Each model family's effort levels, lowest first - the ladder its five
 *  bars climb. Claude's own levels, and the OpenAI reasoning levels that
 *  Codex, Copilot and OpenCode's OpenAI models use. */
const CLAUDE_EFFORT_LADDER = ["low", "medium", "high", "xhigh", "max"] as const;
const OPENAI_EFFORT_LADDER = ["minimal", "low", "medium", "high", "xhigh"] as const;

/** Claude's context-window variant, `claude-opus-5-5[1m]` - the same model as
 *  far as its catalog and effort ladder go. */
const CONTEXT_VARIANT = /\[([^\]]+)\]$/;

/** The id without a `provider/` prefix or a `[1m]` context variant - the form
 *  the agents' catalogs list. */
export function bareModel(model: string) {
  return model.slice(model.lastIndexOf(PROVIDER_SEPARATOR) + 1).replace(CONTEXT_VARIANT, "");
}

/** The human name for a reported model id, with its context variant when it
 *  has one: `claude-opus-5-5[1m]` → "Opus 5.5 1M". */
export function modelName(model: string) {
  const name = familyName(bareModel(model));
  const contextVariant = CONTEXT_VARIANT.exec(model)?.[1];
  if (contextVariant === undefined) {
    return name;
  }

  return `${name} ${contextVariant.toUpperCase()}`;
}

function familyName(bareId: string) {
  if (!bareId.startsWith(CLAUDE_PREFIX)) {
    return bareId;
  }

  const [family, ...rest] = bareId.slice(CLAUDE_PREFIX.length).split(ID_PART_SEPARATOR);
  const versionParts = rest.filter(part => !RELEASE_DATE.test(part));
  const isClaudeShape = family !== undefined && family !== "" && versionParts.every(part => VERSION_PART.test(part));
  if (!isClaudeShape) {
    return bareId;
  }

  const name = family.charAt(0).toUpperCase() + family.slice(1);
  const isVersioned = versionParts.length > 0;
  return isVersioned ? `${name} ${versionParts.join(VERSION_SEPARATOR)}` : name;
}

/** Where an effort level sits on its model's ladder: `step` of `steps`, 1-based.
 *  `ladder` is the model's own levels from its agent's catalog, when known -
 *  it wins; otherwise the family's usual ladder stands in. Null for a level no
 *  ladder knows, so the caller shows the word instead. */
export function effortStep({ model, effort, ladder = [] }: {
  model: string;
  effort: string;
  ladder?: readonly string[];
}) {
  const found = effortOnLadder({
    model,
    effort,
    ladder
  });
  if (found === null) {
    return null;
  }

  return {
    step: found.iEffort + 1,
    steps: found.ladder.length
  };
}

/** The ladder an effort level sits on, and where - the model's own ladder when
 *  known, else its family's. Null for a level no ladder knows. */
function effortOnLadder({ model, effort, ladder }: {
  model: string;
  effort: string;
  ladder: readonly string[];
}) {
  const isClaudeModel = bareModel(model).startsWith(CLAUDE_PREFIX);
  const familyLadders: readonly (readonly string[])[] = isClaudeModel
    ? [CLAUDE_EFFORT_LADDER, OPENAI_EFFORT_LADDER]
    : [OPENAI_EFFORT_LADDER, CLAUDE_EFFORT_LADDER];
  const hasOwnLadder = ladder.length > 0;
  const ladders = hasOwnLadder ? [ladder, ...familyLadders] : familyLadders;
  for (const candidate of ladders) {
    const iEffort = candidate.indexOf(effort.toLowerCase());
    if (iEffort !== NOT_FOUND) {
      return {
        ladder: candidate,
        iEffort
      };
    }
  }

  return null;
}

/** The effort level one step below `effort` on its ladder, or null at the
 *  bottom or for a level no ladder knows. */
export function lowerEffort({ model, effort, ladder = [] }: {
  model: string;
  effort: string;
  ladder?: readonly string[];
}) {
  const found = effortOnLadder({
    model,
    effort,
    ladder
  });
  if (found === null || found.iEffort === 0) {
    return null;
  }

  return found.ladder[found.iEffort - 1];
}

/** How many distinct effort colours the theme defines (`--effort-tone-1..5`). */
const EFFORT_TONES = 5;

/** Which of the theme's effort colours a level is drawn in, 1 (coolest) to
 *  `EFFORT_TONES` (hottest): its place on its ladder, spread over the tones
 *  so a ladder's lowest and highest always land on the two ends. A one-level
 *  ladder has nowhere to climb, so its only level is the hottest. */
export function effortTone({ step, steps }: {
  step: number;
  steps: number;
}) {
  const hasRange = steps > 1;
  const heat = hasRange ? (step - 1) / (steps - 1) : 1;
  return Math.round(heat * (EFFORT_TONES - 1)) + 1;
}

/** A model's human name and its effort as one line: "Opus 5.5 · high effort". */
export function modelLabel({ name, effort }: {
  name: string;
  effort: string | null;
}) {
  return effort === null ? name : `${name} · ${effort} effort`;
}
