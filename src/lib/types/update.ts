import { z } from "zod";

/** Who keeps this copy up to date (`update.rs`'s `Updater`): PADE itself (a
 *  published exe), Windows (an MSIX install, through its `.appinstaller` feed),
 *  or nobody (a development build). */
export const Updater = z.enum(["pade", "windows", "none"]);

/** PADE's own version and self-update state (`update.rs`): the running version,
 *  a newer one downloaded and waiting to be swapped in, and who updates this
 *  copy. */
export const UpdateStatus = z.object({
  current: z.string(),
  ready: z.string().nullable(),
  updater: Updater
});

export type UpdateStatus = z.infer<typeof UpdateStatus>;
