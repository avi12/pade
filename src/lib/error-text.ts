// One reading of a failed backend call for a status toast.
//
// A refusal from the git seam arrives as git's own stderr: several lines, of
// which the first names the obstacle and the rest are advice for a terminal, and
// a severity prefix (`fatal:`) that reads as a crash rather than the refusal it
// is. This is the one home for turning that into the sentence a toast shows, so
// every surface phrases a failure the same way.

/** Git's severity word at the head of the line — noise once the message is in a
 *  toast that already reads as a failure. */
const SEVERITY_PREFIX = /^(?:fatal|error|warning):\s*/;

/** The one line to show for `error`, or `fallback` when it carries no text. */
export function errorHeadline({ error, fallback }: {
  error: unknown;
  fallback: string;
}): string {
  const text = error instanceof Error ? error.message : String(error);
  const headline = text.split("\n")[0].replace(SEVERITY_PREFIX, "").trim();
  return headline || fallback;
}
