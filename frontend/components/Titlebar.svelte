<script lang="ts">
  import { onMount } from 'svelte';
  import Button from './ui/button.svelte';
  import Icon from './Icon.svelte';
  import {
    onWindowResized,
    windowAction,
    windowMaximized,
  } from '../lib/api';

  let { title }: { title: string } = $props();
  let maximized = $state(false);
  onMount(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      maximized = await windowMaximized();
      unlisten = await onWindowResized(async () => {
        maximized = await windowMaximized();
      });
    })();
    return () => unlisten?.();
  });
</script>

<!-- Double-click to toggle maximize. Keyboard users get the same action
     from the maximize button, so the static-element handler is intentional. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<header
  class="titlebar"
  data-tauri-drag-region
  ondblclick={() => windowAction('toggleMaximize')}
>
  <span class="titlebar-label" data-tauri-drag-region>{title}</span>
  <div class="window-controls">
    <Button
      variant="icon"
      class="window-button"
      aria-label="Minimize"
      onclick={() => windowAction('minimize')}
      ><Icon name="minus" size={15} /></Button
    ><Button
      variant="icon"
      class="window-button"
      aria-label={maximized ? 'Restore' : 'Maximize'}
      onclick={() => windowAction('toggleMaximize')}
      ><Icon name={maximized ? 'copy' : 'square'} size={13} /></Button
    ><Button
      variant="icon"
      class="window-button close"
      aria-label="Close"
      onclick={() => windowAction('close')}
      ><Icon name="close" size={16} /></Button
    >
  </div>
</header>
