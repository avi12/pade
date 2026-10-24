<script lang="ts">
  import { runWindowShortcut } from "@/lib/shell/shortcuts/window-shortcut-actions";
  import { WindowShortcut, windowShortcutFor } from "@/lib/shell/shortcuts/window-shortcuts";

  // The window-level signals the shell follows: page visibility, window focus
  // and the window-level key chords. Renders nothing.
  let { isFocused = $bindable(), onvisible }: {
    /** Whether this window has focus. */
    isFocused: boolean;
    /** The page became visible again - the user switched back to it. */
    onvisible: () => void;
  } = $props();
</script>

<svelte:document
  onvisibilitychange={() => {
    if (!document.hidden) {
      onvisible();
    }
  }}
/>
<!-- Window-level shortcuts, handled in the capture phase: a focused xterm calls
     stopPropagation on keys it handles, so a bubble-phase listener never sees the
     combo while the terminal has focus. -->
<svelte:window
  onblur={() => {
    isFocused = false;
  }}
  onfocus={() => {
    isFocused = true;
  }}
  onkeydowncapture={async e => {
    // The chords themselves live in lib/shell/shortcuts/window-shortcuts.
    const shortcut = windowShortcutFor({
      event: e,
      isDevelopment: import.meta.env.DEV
    });
    if (shortcut === null) {
      return;
    }

    e.preventDefault();

    // Keeps the terminal from also receiving the chord - a reload leaves no
    // terminal to reach.
    if (shortcut !== WindowShortcut.Reload) {
      e.stopPropagation();
    }

    await runWindowShortcut(shortcut);
  }}
/>
