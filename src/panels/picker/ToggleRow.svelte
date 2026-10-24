<script lang="ts">
  import Icon from "@/lib/Icon.svelte";

  // One home for the On-launch checkbox row: the box chrome, the tick and the
  // caption, so a new toggle is a line of markup rather than another copy.
  const { checked, onchange, caption }: {
    checked: boolean;
    onchange: (on: boolean) => void;
    caption: string;
  } = $props();
</script>

<label class="check">
  <span class="checkbox">
    <input {checked} onchange={e => onchange(e.currentTarget.checked)} type="checkbox" />
    <span class="box" aria-hidden="true"><Icon name="check" /></span>
  </span>
  <span>{caption}</span>
</label>

<style>
  .check {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 0.8125rem;
    cursor: pointer;
  }
</style>
