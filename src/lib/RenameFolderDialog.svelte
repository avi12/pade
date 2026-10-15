<script lang="ts">
  import NameDialog from "@/lib/NameDialog.svelte";
  import { baseName } from "@/lib/paths";
  import { ProjectName } from "@/lib/validate";
  import type { Relocator } from "@/lib/workspace-relocate";

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
</script>

<NameDialog
  busyLabel="Renaming…"
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
