// PADE's own self-update, as this window sees it (the watch itself runs in
// `update.rs`): the running version, a newer release downloaded and ready to
// swap in, and the two things the user can do about it - check now, or restart
// into the ready release. A release that turns ready is announced with a toast.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import type { UpdateStatus } from "@/lib/types";
import { errorMessage } from "@/lib/util/errors";

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
  await onMessage(EventType.updateReady, ({ data }) => {
    status = data;
    showToast(`PADE ${data.ready ?? ""} is ready - it installs when PADE closes`);
  });
  status = await sendMessage(Command.updateStatus);
}

/** Look for a newer release now; a failure is shown, not thrown. */
export async function checkForUpdate() {
  isChecking = true;
  try {
    status = await sendMessage(Command.updateCheck);
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

/** Swap the ready release in and restart into it. */
export async function restartToUpdate() {
  await sendMessage(Command.updateRestart);
}
