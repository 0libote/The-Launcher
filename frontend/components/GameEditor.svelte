<script lang="ts">
  import { untrack, onDestroy } from 'svelte';
  import Button from './ui/button.svelte';
  import Modal from './Modal.svelte';
  import Icon from './Icon.svelte';
  import { emptyMetadata, type Game } from '../lib/model';
  let {
    game = null,
    close,
    save,
    pick,
  }: {
    game?: Game | null;
    close: () => void;
    save: (game: Game) => Promise<void>;
    pick: (kind: string) => Promise<string | null>;
  } = $props();
  const initial = untrack(() => game);
  let name = $state(initial?.name ?? '');
  let path = $state(
    initial?.target.kind === 'executable' ? initial.target.path : '',
  );
  let args = $state(
    initial?.target.kind === 'executable'
      ? initial.target.arguments.join('\n')
      : '',
  );
  let directory = $state(
    initial?.target.kind === 'executable'
      ? (initial.target.working_directory ?? '')
      : '',
  );
  let tags = $state(initial?.metadata.tags.join(', ') ?? '');
  let series = $state(initial?.metadata.series ?? '');
  let error = $state('');
  let saving = $state(false);
  let alive = true;
  onDestroy(() => (alive = false));
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    error = '';
    saving = true;
    try {
      if (!name.trim()) throw new Error('Enter a game name.');
      if ((!game || game.target.kind === 'executable') && !path.trim())
        throw new Error('Choose the game executable.');
      await save({
        ...(game ?? {
          favorite: false,
          hidden: false,
          installed: true,
          last_launched: null,
          metadata: emptyMetadata(),
        }),
        name: name.trim(),
        target:
          game && game.target.kind !== 'executable'
            ? game.target
            : {
                kind: 'executable',
                path: path.trim(),
                arguments: args.split('\n').filter((line) => line.length > 0),
                working_directory: directory.trim() || null,
              },
        metadata: {
          ...(game?.metadata ?? emptyMetadata()),
          tags: tags
            .split(',')
            .map((tag) => tag.trim())
            .filter(Boolean),
          series: series.trim(),
        },
      });
      if (alive) close();
    } catch (e) {
      error = String(e instanceof Error ? e.message : e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={game ? 'Edit game' : 'Add a game'}
  close={() => {
    if (!saving) close();
  }}
>
  <form onsubmit={submit}>
    <label
      >Game name<input
        bind:value={name}
        placeholder="What are we playing?"
        required
        maxlength="256"
      /></label
    >
    {#if !game || game.target.kind === 'executable'}
      <label
        >Executable
        <div class="input-action">
          <input
            aria-label="Executable"
            bind:value={path}
            placeholder="Choose the game’s executable"
            required
          /><Button
            type="button"
            variant="secondary"
            onclick={async () => {
              try {
                path = (await pick('executable')) ?? path;
              } catch (e) {
                error = String(e);
              }
            }}><Icon name="folder" />Browse</Button
          >
        </div></label
      >
      <details>
        <summary>Launch options</summary><label
          >Arguments<textarea
            bind:value={args}
            rows="3"
            placeholder="One argument per line"></textarea></label
        >
        <p class="field-hint">
          One argument per line. Leave values with spaces unquoted.
        </p>
        <label
          >Working directory
          <div class="input-action">
            <input
              bind:value={directory}
              placeholder="Use the executable’s folder"
            /><Button
              type="button"
              variant="secondary"
              onclick={async () => {
                try {
                  directory = (await pick('folder')) ?? directory;
                } catch (e) {
                  error = String(e);
                }
              }}>Browse</Button
            >
          </div></label
        >
      </details>
    {/if}
    <div class="form-row">
      <label
        >Tags<input
          bind:value={tags}
          placeholder="Cozy, backlog, couch co-op"
        /></label
      ><label>Series<input bind:value={series} placeholder="Optional" /></label>
    </div>
    {#if error}<p class="inline-error" role="alert">{error}</p>{/if}
    <div class="form-actions">
      <Button
        type="button"
        variant="secondary"
        disabled={saving}
        onclick={close}>Cancel</Button
      ><Button disabled={saving}
        >{saving ? 'Saving…' : 'Save game'}</Button
      >
    </div>
  </form>
</Modal>
