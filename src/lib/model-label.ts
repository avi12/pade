// A model id as a person reads it: `claude-opus-5-5` → "Opus 5.5",
// `claude-haiku-4-5-20251001` → "Haiku 4.5". Any id that isn't Claude's
// `claude-<family>-<version…>[-<date>]` shape is shown as it was logged.

const CLAUDE_PREFIX = "claude-";
const RELEASE_DATE = /^\d{8}$/;
const VERSION_PART = /^\d+$/;

/** The human name for a logged model id. */
export function modelName(model: string): string {
  if (!model.startsWith(CLAUDE_PREFIX)) {
    return model;
  }

  const [family, ...rest] = model.slice(CLAUDE_PREFIX.length).split("-");
  const versionParts = rest.filter(part => !RELEASE_DATE.test(part));
  const isClaudeShape = family !== undefined && family !== "" && versionParts.every(part => VERSION_PART.test(part));
  if (!isClaudeShape) {
    return model;
  }

  const name = family.charAt(0).toUpperCase() + family.slice(1);
  return versionParts.length === 0 ? name : `${name} ${versionParts.join(".")}`;
}

/** The model and its effort as one short tag: "Opus 5.5 · high". */
export function modelLabel({ model, effort }: {
  model: string;
  effort: string | null;
}): string {
  const name = modelName(model);
  return effort === null ? name : `${name} · ${effort}`;
}
