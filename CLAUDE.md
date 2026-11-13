@AGENTS.md

# Lint exemptions

Each name below is dictated by an outside API, so it is allowed in the naming rule of `.oxlintrc.json` instead of renamed

- `_core`, `_linkProviderService`: xterm private internals the wrapped-link provider reads
- `__TAURI_INTERNALS__`: the global Tauri injects on window, stubbed by tests
- `five_hour`, `usage_pace`, `load_shedding`, `subagent_steps`, `account_round`: snake_case keys of the pacing JSON from the Rust side
