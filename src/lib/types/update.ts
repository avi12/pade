import { z } from "zod";

/** Who keeps this copy up to date (`update.rs`'s `Updater`): nobody, for a
 *  published exe that is not packaged (it is told to install the MSIX), Windows
 *  (an MSIX install, through its `.appinstaller` feed), or nobody and no notice
 *  (a development build). */
export const Updater = z.enum(["unpackaged", "windows", "none"]);

/** PADE's own version and release notice (`update.rs`): the running version, a
 *  newer release with the page to install its package from, and who updates this
 *  copy. */
export const UpdateStatus = z.object({
  current: z.string(),
  newer: z.object({
    version: z.string(),
    releaseUrl: z.string()
  }).nullable(),
  updater: Updater
});

export type UpdateStatus = z.infer<typeof UpdateStatus>;
