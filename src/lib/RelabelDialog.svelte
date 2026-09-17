<script lang="ts">
  import { workspace } from "@/lib/bridge";
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import { errorMessage } from "@/lib/errors";
  import { LabelSource } from "@/lib/types";
  import { fieldError, WorkspaceLabel } from "@/lib/validate";
  import { untrack } from "svelte";

  // Relabel a temp workspace inside PADE: the name the top bar, the switcher and
  // the picker show, never its folder (a live agent holds that as its cwd, which
  // the OS locks against rename). The label lands in settings — the one home every
  // surface reads through `displayName` — and, being the user's, auto-naming never
  // replaces it. One prompt for every entry point: the top-bar switcher opens it
  // `nested` over its menu, the picker as a modal.
  const { path, currentLabel, nested = false, onclose }: {
    path: string;
    /** The label shown today, prefilled for editing ("" when it has none). */
    currentLabel: string;
    nested?: boolean;
    /** Close the prompt — after a save, or on cancel. */
    onclose: () => void;
  } = $props();

  // Seeded once from the label shown when the prompt opened; the field owns it after.
  let value = $state(untrack(() => currentLabel));
  let saving = $state(false);
  let failure = $state<string | null>(null);
  const issue = $derived(
    fieldError({
      schema: WorkspaceLabel,
      raw: value
    })
  );

  async function save() {
    const parsed = WorkspaceLabel.safeParse(value);
    if (!parsed.success) {
      failure = parsed.error.issues[0].message;
      return;
    }

    saving = true;
    failure = null;
    try {
      await workspace.setLabel({
        path,
        name: parsed.data,
        source: LabelSource.enum.manual
      });
      onclose();
    } catch (error) {
      failure = errorMessage({
        error,
        fallback: "Couldn’t relabel that workspace."
      });
    } finally {
      saving = false;
    }
  }
</script>

<ConfirmDialog
  busy={saving}
  busyLabel="Saving…"
  confirmLabel="Save label"
  error={failure ?? issue}
  icon="pencil"
  {nested}
  oncancel={onclose}
  onconfirm={save}
  title="Relabel this workspace"
>
  <form
    class="relabel" onsubmit={async e => {
      e.preventDefault();
      await save();
    }}>
    <p>Only the name PADE shows changes — the folder keeps its path.</p>
    <input
      aria-label="Workspace label"
      autocomplete="off"
      data-initial-focus
      onfocus={e => e.currentTarget.select()}
      placeholder="What are you working on?"
      spellcheck="false"
      bind:value
    />
    <code>{path}</code>
  </form>
</ConfirmDialog>

<style>
  .relabel {
    display: flex;
    flex-direction: column;
    gap: 10px;

    p {
      margin: 0;
    }

    input {
      padding-block: 9px;
      padding-inline: 12px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-medium);
      background: var(--surface-2);
      color: var(--on-surface);
      font: inherit;
      font-weight: 600;
      font-size: 14px;

      &:focus-visible {
        border-color: var(--primary);
        outline: none;
      }
    }

    code {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 11px;
      overflow-wrap: anywhere;
    }
  }
</style>
