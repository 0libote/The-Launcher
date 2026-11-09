<script lang="ts">
  let { name, size = 20 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    library: 'M4 4h5v16H4z M12 4h4v16h-4z M18 5l3-1 4 15-3 1z',
    grid: 'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z',
    list: 'M8 5h13 M8 12h13 M8 19h13 M3 5h.01 M3 12h.01 M3 19h.01',
    shelf: 'M3 20h18 M4 4h6v13H4z M13 5l5-1 3 12-5 1z',
    heart:
      'M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8Z',
    clock: 'M12 8v5l3 2 M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0',
    search: 'M21 21l-5-5 M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0',
    plus: 'M12 5v14 M5 12h14',
    down: 'M6 9l6 6 6-6',
    arrow: 'M5 12h14 M13 6l6 6-6 6',
    play: 'm8 4 12 8-12 8Z',
    close: 'M6 6l12 12 M6 18 18 6',
    settings:
      'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1z',
    folder: 'M3 7V4h6l3 3h9v13H3z',
    download: 'M12 3v12 M7 10l5 5 5-5 M4 16v5h16v-5',
    monitor: 'M3 3h18v13H3z M12 16v5 M7 21h10',
    gamepad:
      'M7 7h10c3 0 5 9 4 11s-3 1-5-2H8c-2 3-4 4-5 2S4 7 7 7 M6 11v4 M4 13h4 M16 11h.01 M19 14h.01',
    eye: 'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12 M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0',
    edit: 'm16 3 5 5-12 12-6 1 1-6Z M13 6l5 5',
    image: 'M3 3h18v18H3z M3 17l6-6 4 4 3-3 5 5 M8 7h.01',
    check: 'm5 12 4 4L19 6',
    refresh: 'M20 7a9 9 0 1 0 1 9 M20 3v5h-5',
    info: 'M12 11v6 M12 7h.01 M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0',
    back: 'M19 12H5 M11 6l-6 6 6 6',
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.65"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"><path d={paths[name] ?? paths.gamepad} /></svg
>
