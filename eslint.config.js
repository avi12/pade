import { svelte } from "lint-config/eslint/svelte";

export default svelte({
  ignores: [".fallow/", ".verify/", ".claude/worktrees/", "src-tauri/target/", "dist/", "release/"]
});
