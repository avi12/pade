<script lang="ts">
  import ConfirmDialog from "@/lib/dialogs/ConfirmDialog.svelte";
  import NameField from "@/lib/dialogs/NameField.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import { errorMessage } from "@/lib/util/errors/errors";
  import { fieldError } from "@/lib/validate";
  import type { Snippet } from "svelte";
  import { untrack } from "svelte";
  import type { z } from "zod";

  // A prompt that asks for one name and saves it: the field validates live
  // against `schema`, the confirm parses through it, and a rejected save stays
  // open with its reason. Shared by the relabel and rename-folder prompts so
  // they behave alike; each supplies only what saving means.
  const {
    schema,
    initialValue,
    title,
    confirmLabel,
    busyLabel,
    label,
    placeholder,
    path,
    nested = false,
    failureFallback,
    onsave,
    onclose,
    children
  }: {
    schema: z.ZodType<string>;
    /** Prefilled once when the prompt opens; the field owns it after. */
    initialValue: string;
    title: string;
    confirmLabel: string;
    busyLabel: string;
    /** The field's accessible name. */
    label: string;
    placeholder?: string;
    path: string;
    nested?: boolean;
    /** The reason shown when a failed save carries no message of its own. */
    failureFallback: string;
    /** Save the parsed name; throws its reason when it can't. */
    onsave: (name: string) => Promise<void>;
    /** Close the prompt - after a save, or on cancel. */
    onclose: () => void;
    /** What the name changes - shown above the field. */
    children: Snippet;
  } = $props();

  const DIALOG_ICON: IconName = "pencil";

  let value = $state(untrack(() => initialValue));
  let isSaving = $state(false);
  let failure = $state<string | null>(null);
  const issue = $derived(
    fieldError({
      schema,
      raw: value
    })
  );

  async function save() {
    const parsed = schema.safeParse(value);
    if (!parsed.success) {
      failure = parsed.error.issues[0].message;
      return;
    }

    isSaving = true;
    failure = null;
    try {
      await onsave(parsed.data);
      onclose();
    } catch (error) {
      failure = errorMessage({
        error,
        fallback: failureFallback
      });
    } finally {
      isSaving = false;
    }
  }
</script>

<ConfirmDialog
  busy={isSaving}
  {busyLabel}
  {confirmLabel}
  error={failure ?? issue}
  icon={DIALOG_ICON}
  {nested}
  oncancel={onclose}
  onconfirm={save}
  {title}
>
  <NameField {label} onsubmit={save} {path} {placeholder} bind:value>
    {@render children()}
  </NameField>
</ConfirmDialog>
