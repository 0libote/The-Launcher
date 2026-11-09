<script lang="ts">
  import { onMount } from 'svelte';
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  let {
    title,
    close,
    children,
    wide = false,
    error = '',
  }: {
    title: string;
    close: () => void;
    children: Snippet;
    wide?: boolean;
    error?: string;
  } = $props();
  let dialog: HTMLDialogElement;
  onMount(() => {
    dialog.showModal();
    return () => dialog.close();
  });
</script>

<dialog
  aria-label={title}
  bind:this={dialog}
  class:wide
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
  onclick={(event) => {
    if (event.target === dialog) close();
  }}
  onkeydown={(event) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      close();
    }
  }}
>
  <div class="modal-heading">
    <h2>{title}</h2>
    <button class="icon-button" aria-label="Close dialog" onclick={close}
      ><Icon name="close" /></button
    >
  </div>
  <div class="modal-content">
    {@render children()}
    {#if error}<p class="inline-error" role="alert">{error}</p>{/if}
  </div>
</dialog>
