import { FileTone, fileTypeBadge } from "@/lib/file-type";
import { describe, expect, it } from "vitest";

describe("fileTypeBadge", () => {
  it("labels, tones, and logos known source extensions", () => {
    expect(fileTypeBadge("src/App.svelte")).toEqual({
      label: "SV",
      tone: FileTone.Svelte,
      icon: "svelte"
    });
    expect(fileTypeBadge("src/lib/bridge.ts")).toEqual({
      label: "TS",
      tone: FileTone.TypeScript,
      icon: "typescript"
    });
    expect(fileTypeBadge("src-tauri/src/watcher.rs")).toEqual({
      label: "RS",
      tone: FileTone.Rust,
      icon: "rust"
    });
    expect(fileTypeBadge("src/theme.css")).toEqual({
      label: "CSS",
      tone: FileTone.Style,
      icon: "css"
    });
    expect(fileTypeBadge("services/api/usage.py")).toEqual({
      label: "PY",
      tone: FileTone.Python,
      icon: "python"
    });
  });

  it("reads the extension from a Windows path", () => {
    expect(fileTypeBadge("C:\\repos\\pade\\src\\lib\\diff.ts").tone).toBe(FileTone.TypeScript);
  });

  it("is case-insensitive on the extension", () => {
    expect(fileTypeBadge("README.MD")).toEqual({
      label: "MD",
      tone: FileTone.Doc,
      icon: "markdown"
    });
  });

  it("falls back to a neutral chip of the uppercased extension", () => {
    expect(fileTypeBadge("build/output.wasm")).toEqual({
      label: "WASM",
      tone: FileTone.Neutral
    });
  });

  it("handles a dotfile and an extensionless file", () => {
    expect(fileTypeBadge(".gitignore")).toEqual({
      label: "GIT",
      tone: FileTone.Neutral
    });
    expect(fileTypeBadge("Makefile")).toEqual({
      label: "MAK",
      tone: FileTone.Neutral
    });
  });
});
