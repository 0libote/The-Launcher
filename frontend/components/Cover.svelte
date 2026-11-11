<script lang="ts">
  import { desktop } from '../lib/api';
  import { coverThumbnail } from '../lib/artwork';
  import { artSeed, initials, source } from '../lib/model';
  import type { Game } from '../lib/model';
  let {
    game,
    url = null,
    compact = false,
  }: { game: Game; url?: string | null; compact?: boolean } = $props();
  let failed = $state(false);
  let loaded = $state(false);
  let element = $state<HTMLDivElement>();
  let localUrl = $state<string | null>(null);
  // Demo entries hotlink public store artwork; real libraries resolve local
  // art through the desktop thumbnail queue below.
  const remote = $derived(
    game.cover?.startsWith('http://') || game.cover?.startsWith('https://')
      ? game.cover
      : null,
  );
  $effect(() => {
    const path = game.cover;
    const node = element;
    localUrl = null;
    if (!node || !path || !desktop || url) return;
    let alive = true;
    let loaded = false;
    // Load once on first visibility and keep the result: clearing on
    // scroll-out flashes the generated placeholder every time a tile
    // re-enters the viewport. Mounted tiles are batch-capped, so keeping
    // resolved images is bounded.
    const observer = new IntersectionObserver(
      (entries) => {
        if (loaded || !entries.some((entry) => entry.isIntersecting)) return;
        loaded = true;
        coverThumbnail($state.snapshot(game)).then((value) => {
          if (alive) localUrl = value;
        });
        observer.disconnect();
      },
      { rootMargin: '200px' },
    );
    observer.observe(node);
    return () => {
      alive = false;
      observer.disconnect();
    };
  });
  $effect(() => {
    url;
    game.cover;
    failed = false;
    loaded = false;
  });
  // Local art is still decoding: show a neutral shimmer, never the generated
  // stand-in, so nothing on screen pretends to be the cover.
  const waiting = $derived(
    desktop && !failed && !url && !localUrl && !remote && !!game.cover,
  );
</script>

<div
  aria-hidden="true"
  bind:this={element}
  class="cover art-{artSeed(game.name)}"
  class:compact
>
  {#if (url || localUrl || remote) && !failed}<img
      src={url || localUrl || remote || ''}
      alt=""
      loading="lazy"
      decoding="async"
      class:cover-ready={loaded}
      onload={() => (loaded = true)}
      onerror={() => (failed = true)}
    />
  {:else if waiting}<div class="cover-loading" aria-hidden="true"></div>
  {:else}<div class="cover-art" aria-hidden="true">
      <div class="art-orbit"></div>
      <span class="art-initials">{initials(game.name)}</span><span
        class="art-name">{game.name}</span
      ><span class="art-edition">{source(game)} collection</span>
    </div>{/if}
</div>
