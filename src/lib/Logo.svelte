<script lang="ts">
  // PADE brand mark — a three-blade fan on a dark rounded badge. One
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
  <rect fill="#0a0e1a" height="256" rx="56" width="256" x="0" y="0" />
  <g transform="translate(128 128)">
    <path d="M 0 0 C 38 -10 52 -50 18 -92 C -2 -68 -14 -30 0 0 Z" fill="#a7cbff" transform="rotate(0)" />
    <path d="M 0 0 C 38 -10 52 -50 18 -92 C -2 -68 -14 -30 0 0 Z" fill="#4c8dff" transform="rotate(120)" />
    <path d="M 0 0 C 38 -10 52 -50 18 -92 C -2 -68 -14 -30 0 0 Z" fill="#1d4ed8" transform="rotate(240)" />
  </g>
</svg>

<style>
  .logo {
    display: inline-block;
    vertical-align: -0.22em;
    flex: none;
  }
</style>
