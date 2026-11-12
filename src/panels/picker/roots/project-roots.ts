import { normalizePath, parentDirectory } from "@/lib/util/paths";

interface RootsAndRecents {
  roots: string[];
  recentProjects: string[];
}

/** The create-form's best default root: a lone root wins outright; otherwise the
 *  one most recent projects live in (ties keep the earlier root). */
export function popularRoot({ roots, recentProjects }: RootsAndRecents) {
  if (roots.length <= 1) {
    return roots[0] ?? "";
  }

  const recentParents = recentProjects
    .map(parentDirectory)
    .filter((parent): parent is string => parent !== null)
    .map(normalizePath);
  function usage(root: string) {
    return recentParents.filter(parent => parent === normalizePath(root)).length;
  }
  // The sort is stable, so a tie keeps the earlier root first.
  return roots.toSorted((first, second) => usage(second) - usage(first))[0];
}

/** The folders the picker watches so its rows never go stale: each row's PARENT
 *  (a watch holds a handle on the folder it watches, and a handle on a project is
 *  exactly what would stop that project from being deletable), plus every root,
 *  whose children are the projects it lists. Deduplicated. */
export function watchedDirectories({ roots, recentProjects }: RootsAndRecents) {
  return [...new Set(
    [
      ...roots,
      ...roots.map(parentDirectory),
      ...recentProjects.map(parentDirectory)
    ].filter((directory): directory is string => directory !== null)
  )];
}
