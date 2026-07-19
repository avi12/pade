import { isHtmlPath, isImagePath, isMarkdownPath } from "@/lib/preview";
import { describe, expect, it } from "vitest";

describe("isImagePath", () => {
  it("recognises image extensions case-insensitively", () => {
    expect(isImagePath("assets/logo.png")).toBe(true);
    expect(isImagePath("photo.JPG")).toBe(true);
    expect(isImagePath("icon.svg")).toBe(true);
  });

  it("rejects non-images, dotfiles, and extensionless paths", () => {
    expect(isImagePath("src/main.rs")).toBe(false);
    expect(isImagePath(".gitignore")).toBe(false);
    expect(isImagePath("README")).toBe(false);
    expect(isImagePath("notes.md")).toBe(false);
  });
});

describe("isMarkdownPath", () => {
  it("recognises markdown extensions case-insensitively", () => {
    expect(isMarkdownPath("README.md")).toBe(true);
    expect(isMarkdownPath("docs/guide.markdown")).toBe(true);
    expect(isMarkdownPath("CHANGES.MD")).toBe(true);
  });

  it("rejects other extensions", () => {
    expect(isMarkdownPath("index.html")).toBe(false);
    expect(isMarkdownPath("notes.txt")).toBe(false);
    expect(isMarkdownPath("README")).toBe(false);
  });
});

describe("isHtmlPath", () => {
  it("recognises html extensions case-insensitively", () => {
    expect(isHtmlPath("public/index.html")).toBe(true);
    expect(isHtmlPath("page.htm")).toBe(true);
    expect(isHtmlPath("INDEX.HTML")).toBe(true);
  });

  it("rejects other extensions", () => {
    expect(isHtmlPath("README.md")).toBe(false);
    expect(isHtmlPath("data.xml")).toBe(false);
    expect(isHtmlPath("style.css")).toBe(false);
  });
});
