// A model id as a person reads it: `claude-opus-5-5` → "Opus 5.5",
// `claude-haiku-4-5-20251001` → "Haiku 4.5", `anthropic/claude-opus-5` →
// "Opus 5", `openai/gpt-5.6-sol` → "gpt-5.6-sol". Any id that isn't Claude's
// `claude-<family>-<version…>[-<date>]` shape is shown as it was logged, minus
// a provider prefix.

const CLAUDE_PREFIX = "claude-";
const PROVIDER_SEPARATOR = "/";
const RELEASE_DATE = /^\d{8}$/;
const VERSION_PART = /^\d+$/;

/** Each model family's effort levels, lowest first — the ladder its five
 *  bars climb. Claude's own levels, and the OpenAI reasoning levels that
 *  Codex, Copilot and OpenCode's OpenAI models use. */
const CLAUDE_EFFORT_LADDER = ["low", "medium", "high", "xhigh", "max"] as const;
const OPENAI_EFFORT_LADDER = ["minimal", "low", "medium", "high", "xhigh"] as const;

/** The id without a `provider/` prefix. */
function bareModel(model: string): string {
  return model.slice(model.lastIndexOf(PROVIDER_SEPARATOR) + 1);
}

/** The human name for a reported model id. */
export function modelName(model: string): string {
  const bare = bareModel(model);
  if (!bare.startsWith(CLAUDE_PREFIX)) {
    return bare;
  }

  const [family, ...rest] = bare.slice(CLAUDE_PREFIX.length).split("-");
  const versionParts = rest.filter(part => !RELEASE_DATE.test(part));
  const isClaudeShape = family !== undefined && family !== "" && versionParts.every(part => VERSION_PART.test(part));
  if (!isClaudeShape) {
    return bare;
  }

  const name = family.charAt(0).toUpperCase() + family.slice(1);
  return versionParts.length === 0 ? name : `${name} ${versionParts.join(".")}`;
}

/** Where an effort level sits on its model's ladder: `step` of `steps`, 1-based.
 *  Null for a level no ladder knows, so the caller shows the word instead. */
export function effortStep({ model, effort }: {
  model: string;
  effort: string;
}): {
  step: number;
  steps: number;
} | null {
  const isClaudeModel = bareModel(model).startsWith(CLAUDE_PREFIX);
  const ladders: readonly (readonly string[])[] = isClaudeModel
    ? [CLAUDE_EFFORT_LADDER, OPENAI_EFFORT_LADDER]
    : [OPENAI_EFFORT_LADDER, CLAUDE_EFFORT_LADDER];
  for (const ladder of ladders) {
    const index = ladder.indexOf(effort.toLowerCase());
    if (index !== -1) {
      return {
        step: index + 1,
        steps: ladder.length
      };
    }
  }

  return null;
}

/** A model's human name and its effort as one line: "Opus 5.5 · high effort". */
export function modelLabel({ name, effort }: {
  name: string;
  effort: string | null;
}): string {
  return effort === null ? name : `${name} · ${effort} effort`;
}
