<script lang="ts">
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import { errorMessage } from "@/lib/errors";
  import NameField from "@/lib/NameField.svelte";
  import { baseName } from "@/lib/paths";
  import { fieldError, ProjectName } from "@/lib/validate";
  import type { Relocator } from "@/lib/workspace-relocate";
  import { untrack } from "svelte";

  // Rename a project's folder on disk, where it stands. Unlike a relabel, the
  // path itself changes, so everything that knows the folder by path follows it:
  // each agent's memory of it, PADE's settings, the IDEs' recents. The sessions
  // working in it stop for the rename and resume their own conversations after.
  const { path, nested = false, onrename, onclose }: {
    path: string;
    nested?: boolean;
    /** Rename the folder; resolves once it moved, throws its reason otherwise. */
    onrename: Relocator["renameFolder"];
    /** Close the prompt — after the rename, or on cancel. */
    onclose: () => void;
  } = $props();

  // Seeded once from the folder's name when the prompt opened; the field owns it after.
  let value = $state(untrack(() => baseName(path)));
  let renaming = $state(false);
  let failure = $state<string | null>(null);
  const issue = $derived(
    fieldError({
      schema: ProjectName,
      raw: value
    })
  );

  async function rename() {
    const parsed = ProjectName.safeParse(value);
    if (!parsed.success) {
      failure = parsed.error.issues[0].message;
      return;
    }

    const isUnchanged = parsed.data === baseName(path);
    if (isUnchanged) {
      onclose();
      return;
    }

    renaming = true;
    failure = null;
    try {
      await onrename({
        from: path,
        newName: parsed.data
      });
      onclose();
    } catch (error) {
      failure = errorMessage({
        error,
        fallback: "Couldn’t rename that folder."
      });
    } finally {
      renaming = false;
    }
  }
</script>

<ConfirmDialog
  busy={renaming}
  busyLabel="Renaming…"
  confirmLabel="Rename folder"
  error={failure ?? issue}
  icon="pencil"
  {nested}
  oncancel={onclose}
  onconfirm={rename}
  title="Rename this folder"
>
  <NameField label="Folder name" onsubmit={rename} {path} bind:value>
    The folder is renamed on disk. Agents working in it restart there and pick
    up their conversations; their memory of it, PADE’s settings and your
    editors’ recent lists follow it.
  </NameField>
</ConfirmDialog>
