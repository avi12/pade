// The one line Config → Updates shows under its switch: the running version and
// what is happening to it - who updates it when PADE does not, or whether a
// newer release is waiting in the MSIX package.

import { Updater } from "@/lib/types/update";
import type { UpdateStatus } from "@/lib/types/update";

/** Told to a copy that is not packaged once a newer release exists. */
export const INSTALL_MSIX_NOTICE = "PADE now ships as an MSIX package - install it from the latest release";

const OTHER_UPDATER_NOTES = {
  [Updater.enum.windows]: "installed from its Windows package, so updates come through Windows",
  [Updater.enum.none]: "a development build, which never updates itself"
};

/** The status line for `status`, or the waiting line before the backend answers. */
export function updateStatusLine(status: UpdateStatus | null) {
  if (!status) {
    return "Reading the version…";
  }

  if (status.updater !== Updater.enum.unpackaged) {
    return `PADE ${status.current} · ${OTHER_UPDATER_NOTES[status.updater]}`;
  }

  if (status.newer) {
    return `PADE ${status.current} · ${INSTALL_MSIX_NOTICE}`;
  }

  return `PADE ${status.current} · up to date`;
}
