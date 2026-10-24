<script lang="ts" module>
  const BADGE_FILL = "#0a0e1a";
  const BLADE_PATH = "M 0 0 C 38 -10 52 -50 18 -92 C -2 -68 -14 -30 0 0 Z";
  // Lightest to deepest blue, a third of a turn apart.
  const BLADES = [
    {
      fill: "#a7cbff",
      rotation: 0
    },
    {
      fill: "#4c8dff",
      rotation: 120
    },
    {
      fill: "#1d4ed8",
      rotation: 240
    }
  ] as const;
</script>

<script lang="ts">
  // PADE brand mark - a three-blade fan on a dark rounded badge. One
  // authoritative home for the logo (DRY) so every surface renders it
  // identically; sits inline beside the "PADE" wordmark in place of the old ◆.
  const { size = 20 }: {
    size?: number;
  } = $props();
</script>

<svg
  class="logo"
  aria-hidden="true"
  height={size}
  viewBox="0 0 256 256"
  width={size}
  xmlns="http://www.w3.org/2000/svg"
>
  <rect fill={BADGE_FILL} height="256" rx="56" width="256" x="0" y="0" />
  <g transform="translate(128 128)">
    {#each BLADES as blade (blade.rotation)}
      <path d={BLADE_PATH} fill={blade.fill} transform={`rotate(${blade.rotation})`} />
    {/each}
  </g>
</svg>

<style>
  .logo {
    display: inline-block;
    vertical-align: -0.22em;
    flex: none;
  }
</style>
