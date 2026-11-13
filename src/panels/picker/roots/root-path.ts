import type { IconName } from "@/lib/icon-name";
import type { TaggedPathProbe } from "@/lib/types/workspace";
import { normalizePath } from "@/lib/util/paths";
import { FolderPath, parseInput } from "@/lib/validate";

/** The live status banner's tones - each states what adding the typed path
 *  will do. */
const StatusTone = {
  CRITICAL: "critical",
  WARNING: "warning",
  OK: "ok",
  PRIMARY: "primary",
  NEUTRAL: "neutral"
} as const;
type StatusTone = (typeof StatusTone)[keyof typeof StatusTone];

const ROOT_PATH_STATUSES = {
  alreadyRoot: {
    tone: StatusTone.WARNING,
    icon: "folder",
    text: "Already a root folder"
  },
  checking: {
    tone: StatusTone.NEUTRAL,
    icon: "search",
    text: "Checking…"
  },
  file: {
    tone: StatusTone.CRITICAL,
    icon: "alert",
    text: "That path is a file, not a folder"
  },
  existingFolder: {
    tone: StatusTone.OK,
    icon: "check",
    text: "Folder exists - ready to add"
  },
  willCreate: {
    tone: StatusTone.PRIMARY,
    icon: "folderPlus",
    text: "Folder doesn’t exist - PADE will create it"
  },
  invalidLocation: {
    tone: StatusTone.CRITICAL,
    icon: "alert",
    text: "That location doesn’t exist - enter a full folder path"
  }
} satisfies Record<
  string,
  {
    tone: StatusTone;
    icon: IconName;
    text: string;
  }
>;

/** What the typed add-root path is, as far as the disk has answered: whether it
 *  can be added (and whether the folder would be created first), and the live
 *  status line describing it. The probe is async + debounced, so its flags are
 *  only believed once they describe the path currently typed. */
export function assessRootPath({ typed, roots, probe }: { typed: string; roots: string[]; probe: TaggedPathProbe }) {
  const trimmed = typed.trim();
  const hasValue = trimmed.length > 0;
  // A path that differs from an existing root only by case or a trailing
  // separator is the same folder - `C:\repositories\` is already `C:\repositories`.
  const isAlreadyRoot = roots.some(root => normalizePath(root) === normalizePath(trimmed));
  const isSettled = hasValue && probe.path === trimmed;
  const isExistingFolder = isSettled && probe.result.isDir;
  const isFile = isSettled && probe.result.isFile;
  const isMissing = isSettled && !isExistingFolder && !isFile;
  // A not-yet-created folder is addable only when its parent is a real
  // directory - an existence check, not a regex, tells a real location from a
  // stray string.
  const canCreate = isMissing && probe.result.parentExists;
  const isInvalidLocation = isMissing && !probe.result.parentExists;
  const isBlocked = !isSettled || isAlreadyRoot || isFile || isInvalidLocation;
  const path = parseInput({
    schema: FolderPath,
    raw: typed
  });

  return {
    canCreate,
    willCreate: canCreate && !isAlreadyRoot,
    isAddable: !isBlocked,
    /** The validated path to add, or null while adding is blocked. */
    addablePath: isBlocked ? null : path,
    status: rootPathStatus({
      hasValue,
      isAlreadyRoot,
      isSettled,
      isFile,
      isExistingFolder,
      canCreate
    })
  };
}

function rootPathStatus({
  hasValue,
  isAlreadyRoot,
  isSettled,
  isFile,
  isExistingFolder,
  canCreate
}: {
  hasValue: boolean;
  isAlreadyRoot: boolean;
  isSettled: boolean;
  isFile: boolean;
  isExistingFolder: boolean;
  canCreate: boolean;
}) {
  if (!hasValue) {
    return null;
  }

  if (isAlreadyRoot) {
    return ROOT_PATH_STATUSES.alreadyRoot;
  }

  if (!isSettled) {
    return ROOT_PATH_STATUSES.checking;
  }

  if (isFile) {
    return ROOT_PATH_STATUSES.file;
  }

  if (isExistingFolder) {
    return ROOT_PATH_STATUSES.existingFolder;
  }

  if (canCreate) {
    return ROOT_PATH_STATUSES.willCreate;
  }

  return ROOT_PATH_STATUSES.invalidLocation;
}
