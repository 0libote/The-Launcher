<script lang="ts">
  import { coverThumbnail } from '../lib/artwork';
  import type { Game } from '../lib/model';
  let { game, kind }: { game: Game; kind: 'background' | 'logo' } = $props();
  let url = $state<string | null>(null);
  $effect(() => {
    game.metadata[kind];
    let alive = true;
    coverThumbnail($state.snapshot(game), kind).then((value) => {
      if (alive) url = value;
    });
    return () => {
      alive = false;
    };
  });
</script>

{#if url}<img src={url} alt="" decoding="async" />{/if}
