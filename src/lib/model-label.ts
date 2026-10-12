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

/** The id without a `provider/` prefix. */
export function bareModel(model: string) {
  return model.slice(model.lastIndexOf(PROVIDER_SEPARATOR) + 1);
}

/** The human name for a reported model id. */
export function modelName(model: string) {
  const bare = bareModel(model);
  if (!bare.startsWith(CLAUDE_PREFIX)) {
    return bare;
  }

  const [family, ...rest] = bare.slice(CLAUDE_PREFIX.length).split(ID_PART_SEPARATOR);
  const versionParts = rest.filter(part => !RELEASE_DATE.test(part));
  const isClaudeShape = family !== undefined && family !== "" && versionParts.every(part => VERSION_PART.test(part));
  if (!isClaudeShape) {
    return bare;
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
  const isClaudeModel = bareModel(model).startsWith(CLAUDE_PREFIX);
  const familyLadders: readonly (readonly string[])[] = isClaudeModel
    ? [CLAUDE_EFFORT_LADDER, OPENAI_EFFORT_LADDER]
    : [OPENAI_EFFORT_LADDER, CLAUDE_EFFORT_LADDER];
  const hasOwnLadder = ladder.length > 0;
  const ladders = hasOwnLadder ? [ladder, ...familyLadders] : familyLadders;
  for (const candidate of ladders) {
    const index = candidate.indexOf(effort.toLowerCase());
    if (index !== NOT_FOUND) {
      return {
        step: index + 1,
        steps: candidate.length
      };
    }
  }

  return null;
}

/** A model's human name and its effort as one line: "Opus 5.5 · high effort". */
export function modelLabel({ name, effort }: {
  name: string;
  effort: string | null;
}) {
  return effort === null ? name : `${name} · ${effort} effort`;
}
