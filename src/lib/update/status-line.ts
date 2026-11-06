// The one line Config → Updates shows under its switch: the running version and
// what is happening to it - who updates it when PADE does not, or whether a
// newer release is ready.

import { Updater, type UpdateStatus } from "@/lib/types";

const OTHER_UPDATER_NOTES = {
  [Updater.enum.windows]: "installed from its Windows package, so updates come through Windows",
  [Updater.enum.none]: "a development build, which never updates itself"
};

/** The status line for `status`, or the waiting line before the backend answers. */
export function updateStatusLine(status: UpdateStatus | null) {
  if (!status) {
    return "Reading the version…";
  }

  if (status.updater !== Updater.enum.pade) {
    return `PADE ${status.current} · ${OTHER_UPDATER_NOTES[status.updater]}`;
  }

  if (status.ready) {
    return `PADE ${status.current} · ${status.ready} is ready and installs when PADE closes`;
  }

  return `PADE ${status.current} · up to date`;
}
