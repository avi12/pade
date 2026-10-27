// Where the commit view reads its previews: each file as one commit left it,
// so the commit modal shows the same diff, picture, clip or rendered document
// the Change Feed shows for a live change.

import { parseDiff } from "@/lib/code/diff";
import type { PreviewSource } from "@/lib/feed/preview/feed-previews";
import { Command, sendMessage } from "@/lib/ipc/messaging";

/** The preview source for the files of commit `sha` in the repository at `cwd`. */
export function commitPreviewSource({ cwd, sha }: {
  cwd: string;
  sha: string;
}) {
  return {
    diff: async path => parseDiff(
      await sendMessage(Command.vcsCommitDiff, {
        cwd,
        sha,
        path
      })
    ),
    image: path => sendMessage(Command.vcsCommitImage, {
      cwd,
      sha,
      path
    }),
    video: path => sendMessage(Command.vcsCommitVideo, {
      cwd,
      sha,
      path
    }),
    text: path => sendMessage(Command.vcsCommitText, {
      cwd,
      sha,
      path
    })
  } satisfies PreviewSource;
}
