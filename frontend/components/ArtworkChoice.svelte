<script lang="ts">
  import { onMount } from 'svelte';
  import { command } from '../lib/api';
  let {
    choice,
    select,
    disabled,
  }: {
    choice: { label: string; url: string; source: string };
    select: () => void;
    disabled: boolean;
  } = $props();
  let preview = $state<string | null>(null);
  let error = $state('');
  let element: HTMLButtonElement;
  onMount(() => {
    let alive = true;
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) return;
      observer.disconnect();
      command<string>('preview_artwork', { choice })
        .then((url) => {
          if (alive) preview = url;
        })
        .catch((e) => {
          if (alive) error = String(e);
        });
    });
    observer.observe(element);
    return () => {
      alive = false;
      observer.disconnect();
    };
  });
</script>

<button
  bind:this={element}
  class="artwork-choice"
  {disabled}
  onclick={select}
  title={error || choice.source}
>
  {#if preview}<img src={preview} alt="" loading="lazy" />{:else}<span
      class="artwork-placeholder"
      >{error ? 'Preview unavailable' : 'Loading preview…'}</span
    >{/if}
  <span>{choice.label}</span><span class="muted">{choice.source}</span>
</button>
