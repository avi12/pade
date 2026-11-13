import type { TaggedPathProbe } from "@/lib/types/workspace";

/** What Ctrl+Space fills a folder field with: the probe's completion (the next
 *  path segment as far as every folder on disk agrees), but only once the
 *  probe describes the text as typed and the completion reaches further than
 *  it. Null when there is nothing more to fill - the suggestions just open. */
export function nextSegmentCompletion({ typed, probe }: { typed: string; probe: TaggedPathProbe }) {
  const isProbeCurrent = probe.path === typed;
  const { completion } = probe.result;
  const canReachFurther = completion.length > typed.length;
  if (!isProbeCurrent || !canReachFurther) {
    return null;
  }

  return completion;
}
