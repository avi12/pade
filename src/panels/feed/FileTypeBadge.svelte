<script lang="ts">
  import { fileTypeBadge } from "@/lib/feed/file-type";
  import Icon from "@/lib/Icon.svelte";

  // A changed file's type chip: a brand logo when the language has one, else a
  // short label, tinted by the language tone.
  const { path }: { path: string } = $props();

  const badge = $derived(fileTypeBadge(path));
</script>

<span class="file-type tone-{badge.tone}" class:logo={!!badge.icon} aria-hidden="true">
  {#if badge.icon}
    <Icon name={badge.icon} size={18} />
  {:else}
    {badge.label}
  {/if}
</span>

<style>
  .file-type {
    display: inline-grid;
    flex: none;
    place-items: center;
    block-size: 20px;
    min-inline-size: 30px;
    padding-inline: 5px;
    border-radius: 6px;
    background: var(--tone, #6b7280);
    color: #ffffff;
    font-family: var(--font-monospace);
    font-weight: 700;
    font-size: 0.5938rem;
    letter-spacing: 0.02em;

    /* Brand-logo variant: drop the text chip's pill and tint the mark with the
       language tone. Multi-colour marks carry their own baked fills (the tone is
       a no-op there); single-colour marks and the format glyphs take the tone.
       The Icon renders at 1em, so the font-size sets its square. */
    &.logo {
      inline-size: 20px;
      min-inline-size: 0;
      padding: 0;
      background: none;
      color: var(--tone, #6b7280);
      font-size: 1.125rem;
    }
  }

  .tone-typescript {
    --tone: #3b6fe0;
  }

  .tone-javascript {
    --tone: #c9971a;
  }

  .tone-svelte {
    --tone: #e0701c;
  }

  .tone-vue {
    --tone: #41b883;
  }

  .tone-react {
    --tone: #149eca;
  }

  .tone-angular {
    --tone: #dd0031;
  }

  .tone-astro {
    --tone: #bc52ee;
  }

  .tone-laravel {
    --tone: #ff2d20;
  }

  .tone-rails {
    --tone: #cc0000;
  }

  .tone-php {
    --tone: #777bb3;
  }

  .tone-java {
    --tone: #e76f00;
  }

  .tone-csharp {
    --tone: #9b4f96;
  }

  .tone-ruby {
    --tone: #cc342d;
  }

  .tone-kotlin {
    --tone: #7f52ff;
  }

  .tone-swift {
    --tone: #f05138;
  }

  .tone-dart {
    --tone: #0175c2;
  }

  .tone-lua {
    --tone: #5470d6;
  }

  .tone-scala {
    --tone: #dc322f;
  }

  .tone-elixir {
    --tone: #8e5bb5;
  }

  .tone-haskell {
    --tone: #5d4f85;
  }

  .tone-r {
    --tone: #276dc3;
  }

  .tone-zig {
    --tone: #f7a41d;
  }

  .tone-perl {
    --tone: #0073a1;
  }

  .tone-powershell {
    --tone: #5391fe;
  }

  .tone-xml {
    --tone: #e37933;
  }

  .tone-sql {
    --tone: #336791;
  }

  .tone-csv {
    --tone: #217346;
  }

  .tone-env {
    --tone: #c9a227;
  }

  .tone-lock {
    --tone: #8a94a6;
  }

  .tone-batch {
    --tone: #4d8f1e;
  }

  .tone-docker {
    --tone: #2496ed;
  }

  .tone-graphql {
    --tone: #e10098;
  }

  .tone-prisma {
    --tone: #5a67d8;
  }

  .tone-gradle {
    --tone: #1f7a8c;
  }

  .tone-npm {
    --tone: #cb3837;
  }

  .tone-git {
    --tone: #f05032;
  }

  .tone-shader {
    --tone: #5586a4;
  }

  .tone-build {
    --tone: #3a8f6b;
  }

  .tone-llvm {
    --tone: #6b8cc4;
  }

  .tone-license {
    --tone: #b08d2b;
  }

  .tone-windows {
    --tone: #0078d4;
  }

  .tone-rust {
    --tone: #c56a1a;
  }

  .tone-cpp {
    --tone: #00599c;
  }

  .tone-c {
    --tone: #5d8fc9;
  }

  .tone-style {
    --tone: #2596be;
  }

  .tone-markup {
    --tone: #d9822b;
  }

  .tone-python {
    --tone: #3572a5;
  }

  .tone-go {
    --tone: #2ba7bd;
  }

  .tone-data {
    --tone: #7a8290;
  }

  .tone-doc {
    --tone: #6b7688;
  }

  .tone-shell {
    --tone: #55607a;
  }

  .tone-image {
    --tone: #a05fd6;
  }

  .tone-neutral {
    --tone: #6b7280;
  }
</style>
