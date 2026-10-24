<script lang="ts">
  import Icon from "@/lib/Icon.svelte";

  // The shared checkbox control (styled once, as `.checkbox`, in theme.css):
  // a native input the box and its drawn tick sit behind. Put it inside the
  // `<label>` that names it.
  const { checked, disabled = false, onchange }: {
    checked: boolean;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
  } = $props();
</script>

<span class="checkbox">
  <input
    {checked}
    {disabled}
    onchange={e => onchange(e.currentTarget.checked)}
    type="checkbox"
  />
  <span class="box" aria-hidden="true"><Icon name="check" /></span>
</span>
