import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { OpenTarget } from "@/lib/types";
import {
  CloneUrl,
  GitSecret,
  GitUsername,
  parseInput,
  ProjectName
} from "@/lib/validate";
import type { z } from "zod";

const CLONE_FAILED = "Clone failed";

/** One HTTPS credential, sent only when the SSH URL has no key to use - a
 *  blank or invalid value is left out rather than sent. */
function credential({ isNeeded, schema, raw }: {
  isNeeded: boolean;
  schema: z.ZodType<string>;
  raw: string;
}) {
  if (!isNeeded) {
    return undefined;
  }

  return parseInput({
    schema,
    raw
  }) ?? undefined;
}

/** Clone a repository into `root` and open it. Resolves to the message to show
 *  under the form - empty on success, or when the typed fields don't validate
 *  (the form's disabled gate already explains those). */
export async function cloneRepository({ rawUrl, rawName, root, needsCredentials, rawUsername, rawPassword, onopen }: {
  rawUrl: string;
  rawName: string;
  root: string;
  /** An SSH-style URL with no key on disk - the backend rewrites it to HTTPS
   *  and authenticates with the username + password instead. */
  needsCredentials: boolean;
  rawUsername: string;
  rawPassword: string;
  onopen: (target: OpenTarget) => void;
}) {
  const url = parseInput({
    schema: CloneUrl,
    raw: rawUrl
  });
  const name = parseInput({
    schema: ProjectName,
    raw: rawName
  });
  if (!url || !name || !root) {
    return "";
  }

  try {
    const path = await sendMessage(Command.vcsClone, {
      url,
      root,
      name,
      username: credential({
        isNeeded: needsCredentials,
        schema: GitUsername,
        raw: rawUsername
      }),
      password: credential({
        isNeeded: needsCredentials,
        schema: GitSecret,
        raw: rawPassword
      })
    });
    onopen({ path });
    return "";
  } catch (error) {
    return typeof error === "string" ? error : CLONE_FAILED;
  }
}
