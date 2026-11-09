<script lang="ts">
  import { Dialog } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import { cn } from '../lib/utils';
  import Button from './ui/button.svelte';
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
</script>

<Dialog.Root
  open
  onOpenChange={(open) => {
    if (!open) close();
  }}
>
  <Dialog.Portal>
    <Dialog.Overlay class="dialog-overlay" />
    <Dialog.Content class={cn('dialog-panel', wide && 'wide')}>
      <div class="modal-heading">
        <Dialog.Title class="modal-title">{title}</Dialog.Title>
        <Button variant="icon" onclick={close} aria-label="Close dialog"
          ><Icon name="close" /></Button
        >
      </div>
      <div class="modal-content">
        {@render children()}
        {#if error}<p class="inline-error" role="alert">{error}</p>{/if}
      </div>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
