import { svelte } from "eslint-config-avi12/svelte";

export default svelte({ ignores: [".fallow/", ".verify/", ".claude/worktrees/", "src-tauri/target/", "dist/", "release/"] });
