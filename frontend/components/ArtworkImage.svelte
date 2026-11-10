<script lang="ts">
  import { coverThumbnail } from '../lib/artwork';
  import type { Game } from '../lib/model';
  let { game, kind }: { game: Game; kind: 'background' | 'logo' } = $props();
  let url = $state<string | null>(null);
  let failed = $state(false);
  // Demo entries hotlink public store artwork; real libraries resolve local
  // art through the desktop thumbnail queue.
  const remote = $derived(
    typeof game.metadata[kind] === 'string' &&
      (game.metadata[kind].startsWith('http://') ||
        game.metadata[kind].startsWith('https://'))
      ? game.metadata[kind]
      : null,
  );
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
  $effect(() => {
    remote;
    failed = false;
  });
</script>

{#if !failed}
  {#if url}<img
      src={url}
      alt=""
      decoding="async"
      onerror={() => (failed = true)}
    />{:else if remote}<img
      src={remote}
      alt=""
      decoding="async"
      onerror={() => (failed = true)}
    />{/if}
{/if}
