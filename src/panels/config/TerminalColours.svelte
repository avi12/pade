<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { appearance, effective, updatePrefs } from "@/lib/prefs.svelte";
  import { Scheme } from "@/lib/types";
  import SchemeMismatchNote from "@/panels/config/SchemeMismatchNote.svelte";
  import TerminalSchemePicker from "@/panels/config/TerminalSchemePicker.svelte";

  // Which Windows Terminal colour scheme paints the terminal. The pick is stored
  // per app scheme, so the terminal can follow light/dark the way the app does.
  // Whether the user is *editing* them per scheme is view state, not a
  // preference: two equal picks and one pick for both are the same
  // configuration, so it is read off them rather than stored alongside and left
  // free to disagree.
  let perScheme = $state(effective.terminalSchemes.light !== effective.terminalSchemes.dark);
  const activeChoice = $derived(effective.terminalSchemes[appearance.scheme] ?? null);

  async function pickFor({
    scheme,
    name
  }: {
    scheme: Scheme;
    name: string | null;
  }): Promise<void> {
    await updatePrefs({
      terminalSchemes: {
        ...effective.terminalSchemes,
        [scheme]: name
      }
    });
  }

  async function pickForBoth(name: string | null): Promise<void> {
    await updatePrefs({
      terminalSchemes: {
        light: name,
        dark: name
      }
    });
  }
</script>

<div class="field">
  <span class="field-text">
    <span class="field-label">Terminal colours</span>
    <span class="field-hint">Windows Terminal schemes — the ones it ships and your own</span>
  </span>
  <label class="check">
    <span class="checkbox">
      <input
        checked={perScheme}
        onchange={async e => {
          perScheme = e.currentTarget.checked;

          // Collapsing back to one pick keeps what is on screen right now, so
          // turning the switch off never silently repaints the terminal.
          if (!perScheme) {
            await pickForBoth(activeChoice);
          }
        }}
        type="checkbox"
      />
      <span class="box" aria-hidden="true"><Icon name="check" /></span>
    </span>
    <span class="field-text">
      <span class="field-label">Match the app theme</span>
      <span class="field-hint">A light scheme by day, a dark one by night</span>
    </span>
  </label>
  {#if perScheme}
    <TerminalSchemePicker
      id="terminal-scheme-light"
      label="Light"
      onpick={name => pickFor({
        scheme: Scheme.enum.light,
        name
      })}
      value={effective.terminalSchemes.light ?? null}
    />
    <SchemeMismatchNote face={Scheme.enum.light} />
    <TerminalSchemePicker
      id="terminal-scheme-dark"
      label="Dark"
      onpick={name => pickFor({
        scheme: Scheme.enum.dark,
        name
      })}
      value={effective.terminalSchemes.dark ?? null}
    />
    <SchemeMismatchNote face={Scheme.enum.dark} />
  {:else}
    <TerminalSchemePicker id="terminal-scheme" label="" onpick={pickForBoth} value={activeChoice} />
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .field-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-inline-size: 0;
  }

  .field-label {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 12px;
  }

  .field-hint {
    color: var(--on-surface-variant);
    font-size: 11px;
  }

  /* The shared 20px .checkbox (theme.css) beside its label. */
  .check {
    display: flex;
    gap: 10px;
    align-items: center;
    min-inline-size: 0;
    cursor: pointer;
  }
</style>
