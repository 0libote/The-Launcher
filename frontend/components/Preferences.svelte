<script lang="ts">
  import { untrack, onDestroy } from 'svelte';
  import Modal from './Modal.svelte';
  import type { Workspace, Settings, Skin } from '../lib/model';
  import Icon from './Icon.svelte';
  let {
    workspace,
    settings,
    close,
    save,
    backup,
    restore,
    folder,
    preview,
    fill,
  }: {
    workspace: Workspace;
    settings: Settings;
    close: () => void;
    save: (workspace: Workspace, settings: Settings) => Promise<void>;
    backup: () => Promise<void>;
    restore: () => Promise<void>;
    folder: () => void;
    preview: (workspace: Workspace) => void;
    fill: () => void;
  } = $props();
  let draft = $state<Workspace>(
    untrack(() => structuredClone($state.snapshot(workspace))),
  );
  let prefs = $state<Settings>(
    untrack(() => structuredClone($state.snapshot(settings))),
  );
  $effect(() => preview($state.snapshot(draft)));
  let active = $derived(draft.layouts[draft.active]);
  let tab = $state('Appearance');
  let error = $state('');
  let pending = $state(false);
  let alive = true;
  onDestroy(() => (alive = false));
  const skinKeys: (keyof Skin)[] = [
    'background',
    'navigation',
    'surface',
    'raised',
    'text',
    'muted',
    'faint',
    'line',
    'accent',
    'selection',
  ];
  const hex = (rgb: number[]) =>
    '#' + rgb.map((c) => c.toString(16).padStart(2, '0')).join('');
  async function action(fn: () => Promise<void>) {
    error = '';
    pending = true;
    try {
      await fn();
    } catch (e) {
      error = String(e);
    } finally {
      pending = false;
    }
  }
  function duplicate() {
    if (draft.layouts.length >= 32) {
      error = 'You can save up to 32 layouts.';
      return;
    }
    let n = 2;
    let name = `${active.name} ${n}`;
    while (
      draft.layouts.some(
        (preset) => preset.name.toLowerCase() === name.toLowerCase(),
      )
    )
      name = `${active.name} ${++n}`;
    draft.layouts.push({ ...structuredClone($state.snapshot(active)), name });
    draft.active = draft.layouts.length - 1;
  }
</script>

<Modal
  title="Make yourself at home"
  close={() => {
    if (!pending) close();
  }}
  wide
>
  <div class="tab-bar" role="tablist" aria-label="Settings sections">
    {#each ['Appearance', 'Metadata', 'Library & backups'] as item}<button
        role="tab"
        aria-selected={tab === item}
        class:active={tab === item}
        onclick={() => (tab = item)}>{item}</button
      >{/each}
  </div>
  {#if tab === 'Appearance'}
    <div class="form-row">
      <label
        >Saved layout<select bind:value={draft.active}
          >{#each draft.layouts as preset, i}<option value={i}
              >{preset.name}</option
            >{/each}</select
        ></label
      ><button class="secondary align-end" onclick={duplicate}
        ><Icon name="plus" />Duplicate</button
      >
    </div>
    <label>Layout name<input bind:value={active.name} maxlength="64" /></label>
    <div class="form-row">
      <label
        >Navigation<select
          aria-label="Navigation"
          bind:value={active.layout.navigation}
          ><option value="left">Left</option><option value="right">Right</option
          ><option value="hidden">Hidden</option></select
        ></label
      ><label
        >Game details<select bind:value={active.layout.details}
          ><option value="modal">Dialog</option><option value="right"
            >Side panel</option
          ><option value="hidden">Open on request</option></select
        ></label
      >
    </div>
    <div class="form-row">
      <label
        >Toolbar<select bind:value={active.layout.tools}
          ><option value="top">Above library</option><option value="bottom"
            >Below library</option
          ></select
        ></label
      ><label
        >Default view<select bind:value={active.layout.view}
          ><option value="grid">Grid</option><option value="list">List</option
          ><option value="shelf">Shelf</option></select
        ></label
      >
    </div>
    <label
      >Cover size <span class="muted">{active.layout.tile_width}px</span><input
        type="range"
        min="130"
        max="320"
        step="10"
        bind:value={active.layout.tile_width}
      /></label
    >
    <div class="form-row">
      <label
        >Navigation width<input
          type="range"
          min="160"
          max="280"
          step="4"
          bind:value={active.layout.navigation_width}
        /></label
      ><label
        >Details width<input
          type="range"
          min="220"
          max="340"
          step="4"
          bind:value={active.layout.details_width}
        /></label
      >
    </div>
    <label class="check-label"
      ><input type="checkbox" bind:checked={active.layout.show_shortcuts} />Show
      keyboard hints</label
    >
    <div class="color-grid">
      {#each skinKeys as key}<label
          >{key}<input
            type="color"
            value={hex(active.skin[key])}
            oninput={(event) => {
              const value = event.currentTarget.value;
              active.skin[key] = [1, 3, 5].map((i) =>
                parseInt(value.slice(i, i + 2), 16),
              ) as [number, number, number];
            }}
          /></label
        >{/each}
    </div>
  {:else if tab === 'Metadata'}
    <p class="muted">
      Steam supplies public game information. Account imports and metadata are
      separate.
    </p>
    <label class="check-label"
      ><input type="checkbox" bind:checked={prefs.automatic} />Fill missing
      metadata after imports and saves</label
    >
    {#each [['download_text', 'Descriptions and game information'], ['download_cover', 'Cover artwork'], ['download_background', 'Background artwork'], ['download_logo', 'Game logos']] as [key, label]}
      <label class="check-label"
        ><input
          type="checkbox"
          checked={Boolean(prefs[key as keyof Settings])}
          onchange={(event) =>
            (prefs = { ...prefs, [key]: event.currentTarget.checked })}
        />{label}</label
      >
    {/each}
    <label
      >Artwork provider<select bind:value={prefs.artwork_provider}
        ><option value="Steam">Steam official</option><option
          value="SteamGridDb">SteamGridDB, with Steam fallback</option
        ></select
      ></label
    >
    <p class="field-hint">
      SteamGridDB asks for a session key when fetching artwork. Keys are never
      saved to the library.
    </p>
    <button class="secondary" onclick={fill}
      ><Icon name="refresh" />Fill missing information for library</button
    >
    <p class="field-hint">
      Uses your saved download preferences. You can stop the queue from the
      status bar.
    </p>
  {:else}
    <p class="muted">
      Keep a copy of your collection before moving devices or making larger
      changes. Backups contain library entries, not game files or artwork files.
    </p>
    <div class="backup-actions">
      <button
        class="secondary"
        disabled={pending}
        onclick={() => action(backup)}
        ><Icon name="download" />Export library backup</button
      ><button
        class="secondary"
        disabled={pending}
        onclick={() => action(restore)}
        ><Icon name="refresh" />Restore a backup</button
      ><button class="secondary" onclick={folder}
        ><Icon name="folder" />Open library folder</button
      >
    </div>
  {/if}
  {#if error}<p class="inline-error" role="alert">{error}</p>{/if}
  <div class="form-actions">
    <button class="secondary" disabled={pending} onclick={close}>Cancel</button
    ><button
      class="primary"
      disabled={pending}
      onclick={() =>
        action(async () => {
          await save(draft, prefs);
          if (alive) close();
        })}>{pending ? 'Saving…' : 'Save preferences'}</button
    >
  </div>
</Modal>
