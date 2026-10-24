<script lang="ts">
  import NameDialog from "@/lib/dialogs/NameDialog.svelte";
  import { EventType, onMessage } from "@/lib/ipc/messaging";
  import type { RelocateStep } from "@/lib/types";
  import { baseName, normalizePath } from "@/lib/util/paths";
  import { ProjectName } from "@/lib/validate";
  import { RELOCATE_STEP_TEXT } from "@/lib/workspace/relocate/relocate-steps";
  import type { Relocator } from "@/lib/workspace/relocate/workspace-relocate";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  // Rename a project's folder on disk, where it stands. Unlike a relabel, the
  // path itself changes, so everything that knows the folder by path follows it:
  // each agent's memory of it, PADE's settings, the IDEs' recents. The sessions
  // working in it stop for the rename and resume their own conversations after.
  const { path, nested = false, onrename, onclose }: {
    path: string;
    nested?: boolean;
    /** Rename the folder; resolves once it moved, throws its reason otherwise. */
    onrename: Relocator["renameFolder"];
    /** Close the prompt - after the rename, or on cancel. */
    onclose: () => void;
  } = $props();

  // A large project takes a while to follow - every agent's memory, the
  // editors' lists, thousands of package links - so the busy button names the
  // step the move is on rather than one "Renaming…" that reads as stuck.
  let step = $state<RelocateStep | null>(null);
  let unlistenProgress: UnlistenFn | undefined;

  onMount(async () => {
    unlistenProgress = await onMessage(EventType.relocateProgress, ({ data }) => {
      const isThisFolder = normalizePath(data.from) === normalizePath(path);
      if (isThisFolder) {
        step = data.step;
      }
    });
  });

  onDestroy(() => {
    unlistenProgress?.();
  });
</script>

<NameDialog
  busyLabel={step === null ? "Renaming…" : RELOCATE_STEP_TEXT[step]}
  confirmLabel="Rename folder"
  failureFallback="Couldn’t rename that folder"
  initialValue={baseName(path)}
  label="Folder name"
  {nested}
  {onclose}
  onsave={async newName => {
    const isUnchanged = newName === baseName(path);
    if (isUnchanged) {
      return;
    }

    await onrename({
      from: path,
      newName
    });
  }}
  {path}
  schema={ProjectName}
  title="Rename this folder"
>
  The folder is renamed on disk. Agents working in it restart there and pick
  up their conversations; their memory of it, PADE’s settings and your
  editors’ recent lists follow it.
</NameDialog>
