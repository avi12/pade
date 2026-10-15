import type { ConfigFile } from "@/lib/types";
import { normalizePath } from "@/lib/util/paths";

/** Whether a changed path is one of the listed config files. A file is listed by
 *  its project-relative path, a change arrives as any path ending in it; both
 *  sides compare separator- and case-insensitively. */
export function isListedConfigFile({ changedPath, files }: {
  changedPath: string;
  files: readonly ConfigFile[];
}) {
  const changed = normalizePath(changedPath).toLowerCase();
  return files.some(file => {
    const rel = normalizePath(file.rel).toLowerCase();
    return changed === rel || changed.endsWith(`/${rel}`);
  });
}
