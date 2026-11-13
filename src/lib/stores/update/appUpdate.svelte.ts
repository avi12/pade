// The "install the MSIX" notice, as this window sees it (the watch itself runs in
// `update.rs`): the running version and a newer release, if one was found. A
// release that turns up is announced with a toast.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import type { UpdateStatus } from "@/lib/types/update";
import { INSTALL_MSIX_NOTICE } from "@/lib/update/status-line";
import { errorMessage } from "@/lib/util/errors/errors";

let status = $state<UpdateStatus | null>(null);
let isChecking = $state(false);
let isWatching = false;

/** The update state (reactive) - null until the backend first answers. */
export const appUpdate = {
  get status() {
    return status;
  },
  get isChecking() {
    return isChecking;
  }
};

/** Read the state once and follow the backend's announcements. */
export async function watchAppUpdate() {
  if (isWatching) {
    return;
  }

  isWatching = true;
  await onMessage(EventType.UPDATE_AVAILABLE, ({ data }) => {
    status = data;
    showToast(INSTALL_MSIX_NOTICE);
  });
  status = await sendMessage(Command.UPDATE_STATUS);
}

/** Look for a newer release now; a failure is shown, not thrown. */
export async function checkForUpdate() {
  isChecking = true;
  try {
    status = await sendMessage(Command.UPDATE_CHECK);
  } catch (error) {
    showToast(
      `Could not check for updates - ${errorMessage({
        error,
        fallback: "no answer from GitHub"
      })}`
    );
  } finally {
    isChecking = false;
  }
}
