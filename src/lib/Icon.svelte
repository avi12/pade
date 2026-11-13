<script lang="ts" module>
  import type { IconName } from "@/lib/icon-name";

  const SVG_EXTENSION = ".svg";
  const SVG_CLOSING_TAG = "</svg>";

  const ICON_FILES = import.meta.glob<string>("./icons/**/*.svg", {
    query: "?raw",
    import: "default",
    eager: true
  });

  // Drop each file's outer <svg …> wrapper - the template supplies it - leaving
  // just the body to inject. No regex: the first '>' closes the opening tag (SVG
  // path data and attribute values never contain one).
  function iconBody(svg: string) {
    const bodyStart = svg.indexOf(">") + 1;
    return svg.slice(bodyStart, svg.lastIndexOf(SVG_CLOSING_TAG)).trim();
  }

  function iconName(path: string) {
    const base = path.slice(path.lastIndexOf("/") + 1);
    return base.slice(0, base.indexOf(SVG_EXTENSION));
  }

  // A name is the file's base name wherever it sits under ./icons, so two files
  // sharing one would silently shadow each other - refuse to load instead.
  function iconsByName(files: Record<string, string>) {
    const icons: Record<string, string> = {};
    for (const [path, svg] of Object.entries(files)) {
      const name = iconName(path);
      if (Object.hasOwn(icons, name)) {
        throw new Error(`Duplicate icon name "${name}": ${path} clashes with another file in ./icons`);
      }

      icons[name] = iconBody(svg);
    }

    return icons;
  }

  const ICONS = iconsByName(ICON_FILES);
</script>

<script lang="ts">
  const { name, size = 16 }: {
    name: IconName;
    size?: number;
  } = $props();
</script>

<svg
  class="icon"
  aria-hidden="true"
  fill="none"
  height={size}
  stroke="currentColor"
  stroke-linecap="round"
  stroke-linejoin="round"
  stroke-width="1.9"
  viewBox="0 0 24 24"
  width={size}
>{@html ICONS[name] ?? ""}</svg>

<style>
  .icon {
    vertical-align: -0.15em;
    flex: none;
    block-size: 1em;
    inline-size: 1em;
  }
</style>
