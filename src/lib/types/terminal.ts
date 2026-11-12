import { z } from "zod";

/** One runnable task parsed from a project manifest. */
const Task = z.object({
  name: z.string(),
  command: z.string()
});

/** The tasks extracted from one manifest (package.json, Cargo.toml, …). */
export const TaskGroup = z.object({
  manifest: z.string(),
  dir: z.string(),
  kind: z.enum([
    "npm",
    "cargo",
    "make",
    "python",
    "cmake",
    "meson",
    "ninja",
    "bazel",
    "xmake",
    "premake",
    "scons",
    "qmake",
    "dotnet",
    "msbuild",
    "go",
    "gradle",
    "maven"
  ]),
  tasks: z.array(Task)
});

export type TaskGroup = z.infer<typeof TaskGroup>;

/** A token the backend recognises a task manifest by: an exact file name
 *  (`package.json`) or an extension (`.csproj`). The panel lists these in its
 *  empty state, and the catalog matches a changed path against them the same
 *  way the backend matches a directory entry. */
export const ManifestMatch = z.enum(["name", "extension"]);

export const TaskManifestDescriptor = z.discriminatedUnion("match", [
  z.object({
    match: z.literal(ManifestMatch.enum.name),
    value: z.string().min(1)
  }),
  z.object({
    match: z.literal(ManifestMatch.enum.extension),
    value: z.string().min(1)
  })
]);

export type TaskManifestDescriptor = z.infer<typeof TaskManifestDescriptor>;

/** PTY stream event payloads. `seq` is the chunk's position in the session's
 *  stream, which is what lets a terminal attaching mid-flight tell the chunks it
 *  caught live apart from the ones already inside the history it replayed. */
export const PtyChunk = z.object({
  id: z.string(),
  data: z.string(),
  seq: z.number()
});

export type PtyChunk = z.infer<typeof PtyChunk>;

/** A session's replayable output, the sequence number of the last chunk in it, and
 *  whether the program is currently painting the alternate screen - in which case
 *  the history is a stream of framebuffer edits rather than a document, and a
 *  trimmed one cannot be replayed faithfully. */
export const PtyHistory = z.object({
  data: z.string(),
  seq: z.number(),
  alternate: z.boolean()
});

export type PtyHistory = z.infer<typeof PtyHistory>;

export const PtyExit = z.object({ id: z.string() });

export type PtyExit = z.infer<typeof PtyExit>;

/** One live PTY session the backend still hosts (`pty_list`) - the re-attach
 *  roster after a WebView reload. A listed session is alive by construction.
 *  Deliberately no idleness field: the sessions store's status signal is the
 *  one authority on idle (SSOT). */
export const PtySession = z.object({
  id: z.string(),
  /** The agent command it was spawned with (`null` = the default-shell fallback). */
  command: z.string().nullable(),
  /** The directory it was spawned into - its workspace mapping. */
  cwd: z.string().nullable()
});

export type PtySession = z.infer<typeof PtySession>;

/** A tracked task-runner (dock) and its stream event payloads. */
export const RunnerInfo = z.object({
  id: z.string(),
  command: z.string(),
  cwd: z.string().nullable(),
  startedAt: z.number()
});

export type RunnerInfo = z.infer<typeof RunnerInfo>;

export const RunnerStream = z.enum(["stdout", "stderr"]);

export type RunnerStream = z.infer<typeof RunnerStream>;

/** A batch of a runner's output lines from one stream - a burst arrives as one
 *  event, not one per line. */
export const RunnerData = z.object({
  id: z.string(),
  lines: z.array(z.string()),
  stream: RunnerStream
});

export type RunnerData = z.infer<typeof RunnerData>;

export const RunnerExit = z.object({
  id: z.string(),
  code: z.number().nullable()
});

export type RunnerExit = z.infer<typeof RunnerExit>;
