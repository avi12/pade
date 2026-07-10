// Transient status toast — a bottom-center pill that auto-dismisses. Reused by
// the send-from-IDE bridge and window-open actions; one timer, so a new toast
// resets the countdown rather than stacking. SoC: cross-component state lives
// in lib/stores; the app shell only renders `toastText()`.

const TOAST_MS = 2400;

let toast = $state("");
let toastTimer: ReturnType<typeof setTimeout> | undefined;

/** Show `message` in the toast pill, restarting the auto-dismiss countdown. */
export function showToast(message: string): void {
  toast = message;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toast = "";
  }, TOAST_MS);
}

/** The toast currently showing ("" when dismissed). */
export function toastText(): string {
  return toast;
}
