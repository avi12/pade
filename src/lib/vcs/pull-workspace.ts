import { Command, sendMessage } from "@/lib/ipc/messaging";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { errorHeadline } from "@/lib/util/errors/error-text";

/** Fast-forward a workspace from its remote and say how it went in a toast -
 *  the one pull behind the feed's "Sync all" and the Git panel's refresh. A
 *  failure (a diverged branch, no upstream, no network) is toasted, never thrown. */
export async function pullWorkspace(cwd: string) {
  try {
    const outcome = await sendMessage(Command.VCS_PULL, { cwd });
    showToast(outcome.message);
  } catch (error) {
    showToast(
      errorHeadline({
        error,
        fallback: "Sync failed"
      })
    );
  }
}
