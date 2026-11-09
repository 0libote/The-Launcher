<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from './components/Icon.svelte';
  import Button from './components/ui/button.svelte';
  import Cover from './components/Cover.svelte';
  import Modal from './components/Modal.svelte';
  import GameEditor from './components/GameEditor.svelte';
  import GameDetails from './components/GameDetails.svelte';
  import ArtworkChoice from './components/ArtworkChoice.svelte';
  import Preferences from './components/Preferences.svelte';
  import Titlebar from './components/Titlebar.svelte';
  import {
    command,
    demo,
    desktop,
    demoSnapshot,
    type Snapshot,
  } from './lib/api';
  import {
    defaultWorkspace,
    defaultSettings,
    filterGames,
    identity,
    source,
    lastPlayed,
    t3LightSkin,
    t3DarkSkin,
    isLegacyDefaultSkin,
    type Collection,
    type Game,
    type Library,
    type Sort,
    type View,
    type Workspace,
    type Settings,
  } from './lib/model';
  type Appearance = 'system' | 'light' | 'dark';
  let library = $state<Library>({ schema_version: 1, games: [] });
  let workspace = $state(defaultWorkspace());
  let previewWorkspace = $state<Workspace | null>(null);
  let settings = $state(defaultSettings());
  let appearance = $state<Appearance>(
    typeof localStorage !== 'undefined'
      ? (localStorage.getItem('launcher-appearance') as Appearance) || 'system'
      : 'system',
  );
  const systemDark = () =>
    typeof window !== 'undefined' &&
    window.matchMedia?.('(prefers-color-scheme: dark)').matches;
  let osDark = $state(systemDark());
  const effectiveAppearance = $derived<Exclude<Appearance, 'system'>>(
    appearance === 'system' ? (osDark ? 'dark' : 'light') : appearance,
  );
  let collection = $state<Collection>('All games');
  let query = $state('');
  let sort = $state<Sort>('name');
  let installedOnly = $state(false);
  let renderLimit = $state(80);
  let loadSentinel = $state<HTMLDivElement>();
  let selectedKey = $state<string | null>(null);
  let detailsOpen = $state(false);
  let mobileNav = $state(false);
  let loading = $state(true);
  let fatal = $state('');
  let status = $state('');
  let error = $state('');
  let pending = $state(0);
  let modal = $state<
    | null
    | 'add'
    | 'edit'
    | 'import'
    | 'settings'
    | 'metadata'
    | 'artwork'
    | 'remove'
    | 'restore'
  >(null);
  let modalGeneration = 0;
  $effect(() => {
    modal;
    modalGeneration++;
  });
  let dataPath = $state('');
  let importSource = $state('Steam');
  let importRoot = $state('');
  let accountId = $state('');
  let apiKey = $state('');
  let sessionArtKey = $state('');
  let metadataQuery = $state('');
  let matches = $state<{ app_id: number; name: string }[]>([]);
  let warnings = $state<string[]>([]);
  let imageChoices = $state<{ label: string; url: string; source: string }[]>(
    [],
  );
  let replaceMetadata = $state(false);
  let imageKind = $state('cover');
  let searchInput: HTMLInputElement;
  let content: HTMLElement;
  let grid = $state<HTMLDivElement>();
  const collections: { name: Collection; icon: string }[] = [
    { name: 'All games', icon: 'library' },
    { name: 'Favorites', icon: 'heart' },
    { name: 'Recently played', icon: 'clock' },
  ];
  const stores: Collection[] = ['PC', 'Steam', 'Epic', 'Xbox'];
  const active = $derived(
    (previewWorkspace ?? workspace).layouts[
      (previewWorkspace ?? workspace).active
    ],
  );
  $effect(() => {
    for (const [key, value] of Object.entries(active.skin))
      document.documentElement.style.setProperty(
        `--${key}`,
        `rgb(${value.join(',')})`,
      );
    document.documentElement.dataset.theme = effectiveAppearance;
  });
  function applySkin(dark: boolean) {
    active.skin = dark ? t3DarkSkin() : t3LightSkin();
  }
  function setAppearance(next: Appearance) {
    appearance = next;
    try {
      localStorage.setItem('launcher-appearance', next);
    } catch {
      /* Private browsing can block storage; the theme still applies. */
    }
    const dark = next === 'dark' || (next === 'system' && systemDark());
    applySkin(dark);
  }
  function cycleAppearance() {
    setAppearance(
      appearance === 'light' ? 'dark' : appearance === 'dark' ? 'system' : 'light',
    );
  }
  const view = $derived(active.layout.view);
  const visible = $derived(
    filterGames(library.games, collection, query, sort, installedOnly),
  );
  const rendered = $derived(visible.slice(0, renderLimit));
  $effect(() => {
    collection;
    query;
    sort;
    installedOnly;
    renderLimit = 80;
  });
  $effect(() => {
    const node = loadSentinel;
    if (!node) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) renderLimit += 80;
      },
      { rootMargin: '300px' },
    );
    observer.observe(node);
    return () => observer.disconnect();
  });
  const selected = $derived(
    library.games.find((game) => identity(game) === selectedKey) ?? null,
  );
  const visibleSelected = $derived(
    selected && visible.some((game) => identity(game) === selectedKey)
      ? selected
      : null,
  );
  const count = (name: Collection) =>
    filterGames(library.games, name, '', 'added').length;
  const tokens = $derived(
    Object.entries(active.skin)
      .map(([key, rgb]) => `--${key}:rgb(${rgb.join(',')})`)
      .join(';'),
  );
  const coverUrl = (_game: Game) => null;
  function adopt(snapshot: Snapshot) {
    // Existing installs still carry the pre-T3 navy skin. Migrate those to
    // the matching T3 palette; customized skins are left untouched.
    const dark = effectiveAppearance === 'dark';
    for (const layout of snapshot.workspace.layouts) {
      if (isLegacyDefaultSkin(layout.skin))
        layout.skin = dark ? t3DarkSkin() : t3LightSkin();
    }
    library = snapshot.library;
    workspace = snapshot.workspace;
    settings = snapshot.settings;
    dataPath = snapshot.data_path;
    if (snapshot.warnings?.length) warnings = snapshot.warnings;
  }
  let reloadGeneration = 0;
  async function reload() {
    const generation = ++reloadGeneration;
    const snapshot = await command<Snapshot>('get_snapshot');
    if (generation === reloadGeneration) adopt(snapshot);
  }
  async function task(fn: () => Promise<void>, message = '') {
    error = '';
    pending++;
    try {
      await fn();
      if (message) status = message;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      pending--;
    }
  }
  function persistDemo() {
    if (demo)
      sessionStorage.setItem(
        'launcher-demo',
        JSON.stringify($state.snapshot(library)),
      );
  }
  async function saveGame(game: Game) {
    if (demo) {
      const old = modal === 'edit' ? selectedKey : null;
      if (
        library.games.some(
          (entry) =>
            identity(entry) === identity(game) && identity(entry) !== old,
        )
      )
        throw new Error('This game is already in your library.');
      library = {
        ...library,
        games: old
          ? library.games.map((entry) =>
              identity(entry) === old ? game : entry,
            )
          : [...library.games, game],
      };
      persistDemo();
    } else {
      game = await command<Game>('save_game', {
        game: $state.snapshot(game),
        original: modal === 'edit' ? $state.snapshot(selected?.target) : null,
      });
      await reload();
    }
    selectedKey = identity(game);
    status = 'Game saved';
    if (settings.automatic && !demo) void task(() => automaticMetadata([game]));
  }
  async function updateGame(game: Game, patch: Partial<Game>) {
    if (demo) {
      library = {
        ...library,
        games: library.games.map((entry) =>
          identity(entry) === identity(game) ? { ...entry, ...patch } : entry,
        ),
      };
      persistDemo();
    } else {
      await command('update_game', {
        target: $state.snapshot(game.target),
        patch,
      });
      await reload();
    }
  }
  function chooseCollection(name: Collection) {
    collection = name;
    mobileNav = false;
    detailsOpen = false;
  }
  function chooseGame(game: Game) {
    selectedKey = identity(game);
    detailsOpen = true;
  }
  async function play(game: Game) {
    await task(async () => {
      if (demo)
        throw new Error(
          'Demo games cannot be launched. Add your own games in the desktop app.',
        );
      await command('launch_game', { target: $state.snapshot(game.target) });
      await reload();
    }, 'Launch request sent');
  }
  async function pick(kind: string) {
    return command<string | null>('pick_path', { kind });
  }
  async function setView(next: View) {
    const before = $state.snapshot(workspace);
    active.layout.view = next;
    if (!demo)
      await task(async () => {
        try {
          await command('save_workspace', {
            workspace: $state.snapshot(workspace),
          });
        } catch (e) {
          workspace = before;
          throw e;
        }
      });
  }
  async function importGames() {
    const generation = modalGeneration;
    await task(async () => {
      if (demo)
        throw new Error('Open the desktop app to scan your installed games.');
      const result = await command<{ warnings: string[] }>('import_games', {
        source: importSource.toLowerCase(),
        root: importRoot || null,
        accountId: accountId || null,
        apiKey: apiKey || null,
      });
      warnings = result.warnings;
      apiKey = '';
      await reload();
      if (modal === 'import' && generation === modalGeneration) modal = null;
      if (settings.automatic) await automaticMetadata(library.games);
    }, 'Library import finished');
  }
  let stopMetadata = false;
  let metadataRunning = $state(false);
  const metadataQueue: Game[] = [];
  async function automaticMetadata(games: Game[]) {
    const queued = new Set(metadataQueue.map(identity));
    metadataQueue.push(
      ...games.filter(
        (game) =>
          !game.hidden &&
          !queued.has(identity(game)) &&
          (game.target.kind === 'steam' || game.metadata.steam_app_id),
      ),
    );
    if (metadataRunning) return;
    metadataRunning = true;
    stopMetadata = false;
    try {
      while (metadataQueue.length && !stopMetadata) {
        const game = metadataQueue.shift()!;
        if (
          !library.games.some(
            (entry) => !entry.hidden && identity(entry) === identity(game),
          )
        )
          continue;
        status = `Fetching metadata for ${game.name}… (${metadataQueue.length} remaining)`;
        try {
          const result = await command<{ warnings: string[] }>(
            'fetch_metadata',
            {
              target: $state.snapshot(game.target),
              replace: false,
              key: sessionArtKey,
              linkedId: game.metadata.steam_app_id,
              provider: settings.artwork_provider,
            },
          );
          warnings = [...warnings, ...(result.warnings ?? [])];
          await reload();
          if (
            result.warnings?.some((warning) => /429|rate limit/i.test(warning))
          )
            stopMetadata = true;
        } catch (e) {
          warnings = [...warnings, String(e)];
          stopMetadata = true;
        }
        if (metadataQueue.length && !stopMetadata)
          await new Promise((resolve) => setTimeout(resolve, 500));
      }
      status = stopMetadata ? 'Metadata fetching stopped' : 'Metadata updated';
    } finally {
      metadataRunning = false;
      metadataQueue.length = 0;
    }
  }
  function openMetadata() {
    if (!selected) return;
    metadataQuery = selected.name;
    matches = [];
    replaceMetadata = false;
    modal = 'metadata';
  }
  async function findMetadata() {
    const q = metadataQuery;
    const generation = modalGeneration;
    await task(async () => {
      const results = await command<typeof matches>('search_metadata', {
        query: q,
      });
      if (
        metadataQuery === q &&
        modal === 'metadata' &&
        generation === modalGeneration
      )
        matches = results;
    });
  }
  async function fetchMetadata(id?: number) {
    const generation = modalGeneration;
    const game = selected;
    if (!game) return;
    await task(async () => {
      const result = await command<{ warnings: string[] }>('fetch_metadata', {
        target: $state.snapshot(game.target),
        appId: id ?? null,
        replace: replaceMetadata,
        key: sessionArtKey,
        linkedId: game.metadata.steam_app_id,
        provider: settings.artwork_provider,
      });
      warnings = result.warnings ?? [];
      await reload();
      if (modal === 'metadata' && generation === modalGeneration) modal = null;
    }, 'Metadata updated');
  }
  async function chooseLocalCover() {
    if (!selected) return;
    const game = selected;
    await task(async () => {
      await command('choose_cover', { target: $state.snapshot(game.target) });
      await reload();
    });
  }
  async function browseArtwork() {
    const generation = modalGeneration;
    const kind = imageKind;
    const game = selected;
    if (!game) return;
    await task(async () => {
      const choices = await command<typeof imageChoices>('artwork_choices', {
        target: $state.snapshot(game.target),
        kind,
        key: sessionArtKey,
        linkedId: game.metadata.steam_app_id,
        provider: settings.artwork_provider,
      });
      if (
        selectedKey === identity(game) &&
        modal === 'artwork' &&
        generation === modalGeneration &&
        kind === imageKind
      )
        imageChoices = choices;
    });
  }
  async function useArtwork(choice: {
    label: string;
    url: string;
    source: string;
  }) {
    if (!selected) return;
    const game = selected;
    const generation = modalGeneration;
    await task(async () => {
      await command('set_artwork', {
        target: $state.snapshot(game.target),
        kind: imageKind,
        choice,
      });
      await reload();
      if (modal === 'artwork' && generation === modalGeneration) modal = null;
    }, 'Artwork updated');
  }
  async function removeGame() {
    const generation = modalGeneration;
    if (!selected) return;
    const game = selected;
    await task(async () => {
      if (demo) {
        library = {
          ...library,
          games: library.games.filter(
            (entry) => identity(entry) !== identity(game),
          ),
        };
        persistDemo();
      } else {
        await command('remove_game', { target: $state.snapshot(game.target) });
        await reload();
      }
      if (selectedKey === identity(game)) {
        selectedKey = null;
        detailsOpen = false;
      }
      if (modal === 'remove' && generation === modalGeneration) modal = null;
    }, 'Game removed from library');
  }
  function keyboard(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (!modal) {
        detailsOpen = false;
        mobileNav = false;
      }
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      searchInput?.focus();
      return;
    }
    if (
      event.key.toLowerCase() === 'a' &&
      event.altKey &&
      (event.ctrlKey || event.metaKey)
    ) {
      event.preventDefault();
      cycleAppearance();
      return;
    }
    const target = event.target as HTMLElement;
    const tag = target?.tagName;
    if (
      event.key === '/' &&
      !event.ctrlKey &&
      !event.metaKey &&
      tag !== 'INPUT' &&
      tag !== 'TEXTAREA' &&
      tag !== 'SELECT'
    ) {
      event.preventDefault();
      searchInput?.focus();
      return;
    }
    if (
      modal ||
      (target &&
        ['INPUT', 'TEXTAREA', 'SELECT', 'BUTTON'].includes(target.tagName) &&
        !target.closest('.game-card'))
    )
      return;
    if (!visible.length || detailsOpen) return;
    if (
      ['ArrowRight', 'ArrowLeft', 'ArrowDown', 'ArrowUp'].includes(event.key)
    ) {
      event.preventDefault();
      const columns =
        view === 'list'
          ? 1
          : Math.max(
              1,
              (grid ? getComputedStyle(grid).gridTemplateColumns : '1')
                .toString()
                .split(' ').length,
            );
      const index = Math.max(
        0,
        visible.findIndex((game) => identity(game) === selectedKey),
      );
      const step =
        event.key === 'ArrowRight'
          ? 1
          : event.key === 'ArrowLeft'
            ? -1
            : event.key === 'ArrowDown'
              ? columns
              : -columns;
      const next =
        visible[
          Math.min(
            visible.length - 1,
            Math.max(0, selectedKey ? index + step : 0),
          )
        ];
      if (visible.indexOf(next) >= renderLimit) renderLimit += 80;
      selectedKey = identity(next);
      tick().then(() =>
        grid?.querySelector<HTMLElement>('[data-selected="true"]')?.focus(),
      );
    }
  }
  onMount(() => {
    (async () => {
      try {
        if (demo) adopt(demoSnapshot());
        else if (desktop) await reload();
        else adopt({ library, workspace, settings, data_path: '' });
        const freshSkin = JSON.stringify(active.skin);
        if (
          freshSkin === JSON.stringify(t3LightSkin()) ||
          freshSkin === JSON.stringify(t3DarkSkin())
        )
          applySkin(effectiveAppearance === 'dark');
      } catch (e) {
        fatal = String(e);
      } finally {
        loading = false;
      }
    })();
    const media = window.matchMedia?.('(prefers-color-scheme: dark)');
    const onSystemTheme = () => {
      osDark = systemDark();
      if (appearance !== 'system') return;
      const freshSkin = JSON.stringify(active.skin);
      if (
        freshSkin !== JSON.stringify(t3LightSkin()) &&
        freshSkin !== JSON.stringify(t3DarkSkin())
      )
        return;
      applySkin(systemDark());
    };
    media?.addEventListener?.('change', onSystemTheme);
    // Poll only while a controller is attached; keyboard/mouse work without this timer.
    let frame = 0;
    let lastAction = 0;
    let wasConfirm = false;
    let wasBack = false;
    const controller = (time: number) => {
      const pad = navigator.getGamepads?.().find((pad) => pad?.connected);
      if (pad) {
        const confirm = pad.buttons[0]?.pressed;
        if (confirm && !wasConfirm && !modal) {
          if (detailsOpen && selected) void play(selected);
          else if (selected) detailsOpen = true;
          else if (visible[0]) chooseGame(visible[0]);
        }
        wasConfirm = confirm;
        const back = pad.buttons[1]?.pressed;
        if (back && !wasBack) {
          if (modal) {
            modal = null;
            previewWorkspace = null;
            apiKey = '';
          } else detailsOpen = false;
        }
        wasBack = back;
        if (time - lastAction > 180 && !detailsOpen && !modal) {
          const key =
            pad.buttons[15]?.pressed || pad.axes[0] > 0.5
              ? 'ArrowRight'
              : pad.buttons[14]?.pressed || pad.axes[0] < -0.5
                ? 'ArrowLeft'
                : pad.buttons[13]?.pressed || pad.axes[1] > 0.5
                  ? 'ArrowDown'
                  : pad.buttons[12]?.pressed || pad.axes[1] < -0.5
                    ? 'ArrowUp'
                    : '';
          if (key) {
            keyboard(new KeyboardEvent('keydown', { key }));
            lastAction = time;
          }
        }
        frame = requestAnimationFrame(controller);
      } else frame = 0;
    };
    const connect = () => {
      if (!frame) frame = requestAnimationFrame(controller);
    };
    window.addEventListener('gamepadconnected', connect);
    connect();
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('gamepadconnected', connect);
      media?.removeEventListener?.('change', onSystemTheme);
    };
  });
</script>

<svelte:window onkeydown={keyboard} />
<div
  class="app-shell"
  class:nav-right={active.layout.navigation === 'right'}
  class:nav-hidden={active.layout.navigation === 'hidden'}
  class:has-titlebar={desktop}
  style={`${tokens};--tile-width:${active.layout.tile_width}px;--nav-width:${active.layout.navigation_width}px;--details-width:${active.layout.details_width}px`}
>
  <a class="skip-link" href="#library">Skip to library</a>
  {#if mobileNav}<button
      class="nav-scrim"
      aria-label="Close navigation"
      onclick={() => (mobileNav = false)}
    ></button>{/if}
  <aside
    class="navigation"
    class:mobile-open={mobileNav}
    aria-label="Library navigation"
  >
    <a
      class="brand"
      href="#library"
      onclick={() => chooseCollection('All games')}
      ><span class="brand-symbol"><Icon name="shelf" size={25} /></span><span
        >The Launcher<span class="brand-caption">A home for your games</span
        ></span
      ></a
    >
    <div class="nav-section">Your collection</div>
    <nav aria-label="Collections">
      {#each collections as item}<button
          class:active={collection === item.name}
          onclick={() => chooseCollection(item.name)}
          aria-current={collection === item.name ? 'page' : undefined}
          ><Icon name={item.icon} /><span>{item.name}</span><span
            class="nav-count">{count(item.name)}</span
          ></button
        >{/each}
    </nav>
    <div class="nav-section">Platforms</div>
    <nav aria-label="Platforms">
      {#each stores as store}<button
          class:active={collection === store}
          onclick={() => chooseCollection(store)}
          aria-current={collection === store ? 'page' : undefined}
          ><span class="store-icon"
            >{store === 'PC' ? 'PC' : store.slice(0, 1)}</span
          ><span>{store}</span><span class="nav-count">{count(store)}</span
          ></button
        >{/each}
    </nav>
    <div class="nav-spacer"></div>
    <nav class="nav-bottom" aria-label="Library tools">
      <button
        class:active={collection === 'Hidden'}
        onclick={() => chooseCollection('Hidden')}
        ><Icon name="eye" /><span>Hidden games</span><span class="nav-count"
          >{count('Hidden')}</span
        ></button
      ><button onclick={() => (modal = 'settings')}
        ><Icon name="settings" /><span>Preferences</span></button
      >
    </nav>
    <div class="local-note">
      <Icon name="monitor" size={15} /><span
        >{demo ? 'Demo collection' : 'Your library. Your device.'}</span
      >
    </div>
  </aside>
  <div class="main-shell">
    {#if desktop}<Titlebar
        title={collection === 'All games'
          ? 'The Launcher'
          : `The Launcher — ${collection}`}
      />{/if}
    <header class="topbar">
      <Button
        variant="icon"
        class="nav-toggle"
        onclick={() => (mobileNav = !mobileNav)}
        aria-label="Toggle navigation"><Icon name="library" /></Button
      >
      <div class="breadcrumb">
        Your collection <span>/</span> <strong>{collection}</strong>
      </div>
      <div class="topbar-actions">
        <Button
          variant="icon"
          onclick={cycleAppearance}
          aria-label={`Appearance: ${appearance}. Activate to switch theme`}
          aria-pressed={effectiveAppearance === 'dark'}
          title={`Appearance: ${appearance} (Ctrl+Alt+A)`}
          ><Icon
            name={effectiveAppearance === 'dark' ? 'sun' : 'moon'}
            size={17}
          /></Button
        ><Button
          variant="secondary"
          class="import-button"
          onclick={() => {
            modal = 'import';
            warnings = [];
          }}><Icon name="download" />Import games</Button
        ><Button onclick={() => (modal = 'add')}
          ><Icon name="plus" />Add game</Button
        >
      </div>
    </header>
    {#if demo}<div class="demo-banner" role="note" aria-label="Browser mode">
        <Icon name="info" size={16} /><span
          >Demo collection. Changes stay in this browser tab; launching and
          imports need the desktop app.</span
        >
      </div>{:else if !desktop}<div
        class="demo-banner"
        role="note"
        aria-label="Browser mode"
      >
        <Icon name="info" size={16} /><span
          >Open the desktop app for your local library, or <a href="?demo"
            >explore the demo collection</a
          >.</span
        >
      </div>{/if}
    <div
      class="library-layout"
      class:has-details={detailsOpen &&
        visibleSelected &&
        active.layout.details === 'right'}
    >
      <main id="library" bind:this={content} tabindex="-1">
        <div class="collection-heading">
          <div>
            <h1>
              {collection === 'All games' ? 'Your next adventure.' : collection}
            </h1>
            <p>
              {collection === 'All games'
                ? 'Old favorites. New discoveries. All in one place.'
                : collection === 'Hidden'
                  ? 'Out of sight, still part of your collection.'
                  : 'A little less searching. A little more playing.'}
            </p>
          </div>
          <span class="collection-total"
            >{visible.length} {visible.length === 1 ? 'game' : 'games'}</span
          >
        </div>
        <div
          class="library-controls"
          class:controls-bottom={active.layout.tools === 'bottom'}
        >
          <div class="search-field">
            <Icon name="search" size={18} /><input
              bind:this={searchInput}
              bind:value={query}
              placeholder="Find a game, genre, or tag"
              aria-label="Search games"
            />{#if query}<Button
                variant="icon"
                aria-label="Clear search"
                onclick={() => (query = '')}
                ><Icon name="close" size={15} /></Button
              >{:else}<kbd>/</kbd>{/if}
          </div>
          <div class="view-controls">
            <label class="installed-filter"
              ><input
                type="checkbox"
                bind:checked={installedOnly}
              />Installed</label
            ><label class="sort-control"
              ><span class="sr-only">Sort games</span><select bind:value={sort}
                ><option value="name">Title A–Z</option><option value="recent"
                  >Recently played</option
                ><option value="added">Recently added</option></select
              ></label
            >
            <div class="view-switch" aria-label="Library view">
              {#each [{ value: 'grid', icon: 'grid', label: 'Grid view' }, { value: 'shelf', icon: 'shelf', label: 'Shelf view' }, { value: 'list', icon: 'list', label: 'List view' }] as option}<button
                  class:active={view === option.value}
                  aria-label={option.label}
                  aria-pressed={view === option.value}
                  onclick={() => setView(option.value as View)}
                  ><Icon name={option.icon} size={18} /></button
                >{/each}
            </div>
          </div>
        </div>
        {#if loading}<div class="empty-state">
            <Icon name="shelf" size={44} />
            <h2>Opening your collection…</h2>
          </div>
        {:else if fatal}<div class="empty-state">
            <h2>Your library needs attention</h2>
            <p>{fatal}</p>
            <p>Your existing files have not been overwritten.</p>
          </div>
        {:else if !visible.length}<div class="empty-state">
            <div class="empty-cases">
              <span></span><span><Icon name="gamepad" size={42} /></span><span
              ></span>
            </div>
            <h2>
              {library.games.length
                ? 'No games on this shelf'
                : 'Every collection starts somewhere.'}
            </h2>
            <p>
              {library.games.length
                ? 'Try another search or collection to find what you’re looking for.'
                : 'Bring your PC games together, then make the shelf your own.'}
            </p>
            <div class="empty-actions">
              {#if library.games.length}<Button
                  variant="secondary"
                  onclick={() => {
                    query = '';
                    installedOnly = false;
                    chooseCollection('All games');
                  }}>Show all games</Button
                >{:else}<Button onclick={() => (modal = 'add')}
                  ><Icon name="plus" />Add your first game</Button
                ><Button
                  variant="secondary"
                  onclick={() => (modal = 'import')}
                  >Import a library</Button
                >{/if}
            </div>
          </div>
        {:else}<div
            bind:this={grid}
            class="games"
            class:list-view={view === 'list'}
            class:shelf-view={view === 'shelf'}
            aria-label="Games"
          >
            {#each rendered as game (identity(game))}<div class="game-entry">
                <button
                  class="game-card"
                  class:selected={selectedKey === identity(game)}
                  data-selected={selectedKey === identity(game)}
                  onclick={() => chooseGame(game)}
                  ondblclick={() => play(game)}
                  ><Cover
                    {game}
                    url={coverUrl(game)}
                    compact={view === 'list'}
                  />
                  <div class="game-caption">
                    <span class="game-name">{game.name}</span><span
                      class="game-source"
                      aria-hidden="true"
                      >{source(game)}{view === 'list'
                        ? ` / ${game.metadata.genres[0] || 'Game'}`
                        : ''}</span
                    >
                  </div>
                  {#if view === 'list'}<span
                      class="list-last-played"
                      aria-hidden="true">{lastPlayed(game.last_launched)}</span
                    >{/if}<span
                    aria-hidden="true"
                    class="installation"
                    class:installed={game.installed === true}
                    >{game.installed === true
                      ? 'Installed'
                      : game.installed === false
                        ? 'In library'
                        : 'Local'}</span
                  ></button
                ><button
                  class="favorite-button"
                  class:is-favorite={game.favorite}
                  aria-label={`${game.favorite ? 'Unfavorite' : 'Favorite'} ${game.name}`}
                  aria-pressed={game.favorite}
                  onclick={() =>
                    task(() => updateGame(game, { favorite: !game.favorite }))}
                  ><Icon name="heart" size={16} /></button
                >
              </div>{/each}
          </div>
          {#if rendered.length < visible.length}<div
              class="load-more"
              bind:this={loadSentinel}
            >
              <Button
                variant="secondary"
                onclick={() => (renderLimit += 80)}
                >Show more games ({visible.length - rendered.length} remaining)</Button
              >
            </div>{/if}{/if}
        <footer class="library-footer">
          <span
            ><Icon name="shelf" size={15} />{library.games.filter(
              (game) => !game.hidden,
            ).length} games in your collection</span
          >{#if active.layout.show_shortcuts}<span class="keyboard-hints"
              ><kbd>↑</kbd><kbd>↓</kbd> Browse <kbd>Enter</kbd> Details
              <kbd>Esc</kbd> Back</span
            >{/if}
        </footer>
      </main>
      {#if detailsOpen && visibleSelected && active.layout.details === 'right'}<aside
          class="detail-panel"
        >
          {@render details(visibleSelected)}
        </aside>{/if}
    </div>
    {#if error || status || pending || metadataRunning}<div
        class="status-bar"
        class:error-status={Boolean(error)}
        role={error ? 'alert' : 'status'}
      >
        <Icon
          name={error
            ? 'info'
            : pending || metadataRunning
              ? 'refresh'
              : 'check'}
          size={16}
        /><span
          >{error || (pending && !metadataRunning ? 'Working…' : status)}</span
        >{#if metadataRunning}<Button
            variant="ghost"
            onclick={() => (stopMetadata = true)}>Stop fetching</Button
          >{/if}<Button
          variant="icon"
          aria-label="Dismiss notification"
          onclick={() => {
            error = '';
            status = '';
          }}><Icon name="close" size={16} /></Button
        >
      </div>{/if}
    {#if warnings.length}<details class="warning-tray">
        <summary
          >{warnings.length} import or metadata {warnings.length === 1
            ? 'warning'
            : 'warnings'}</summary
        >{#each warnings as warning}<p>{warning}</p>{/each}
      </details>{/if}
  </div>
</div>

{#snippet details(game: Game)}
  <GameDetails
    {game}
    cover={coverUrl(game)}
    close={() => (detailsOpen = false)}
    play={() => play(game)}
    favorite={() => task(() => updateGame(game, { favorite: !game.favorite }))}
    edit={() => (modal = 'edit')}
    artwork={() => {
      imageChoices = [];
      modal = 'artwork';
    }}
    metadata={openMetadata}
    remove={() => (modal = 'remove')}
    hidden={() =>
      task(async () => {
        await updateGame(game, { hidden: !game.hidden });
        detailsOpen = false;
      })}
    openLink={(index) =>
      task(async () => {
        await command('open_game_link', {
          target: $state.snapshot(game.target),
          index,
        });
      })}
    busy={pending > 0}
  />
{/snippet}
{#if detailsOpen && visibleSelected && active.layout.details !== 'right' && !modal}<Modal
    {error}
    title="Game details"
    close={() => (detailsOpen = false)}
    wide
    ><div class="detail-modal">{@render details(visibleSelected)}</div></Modal
  >{/if}
{#if modal === 'add' || modal === 'edit'}<GameEditor
    game={modal === 'edit' ? selected : null}
    close={() => (modal = null)}
    save={saveGame}
    {pick}
  />
{:else if modal === 'settings'}<Preferences
    {workspace}
    {settings}
    fill={() => {
      modal = null;
      previewWorkspace = null;
      void task(() => automaticMetadata(library.games));
    }}
    preview={(draft) => (previewWorkspace = draft)}
    close={() => {
      modal = null;
      previewWorkspace = null;
    }}
    save={async (nextWorkspace: Workspace, nextSettings: Settings) => {
      if (!demo) {
        await command('save_preferences', {
          workspace: $state.snapshot(nextWorkspace),
          settings: $state.snapshot(nextSettings),
        });
      }
      workspace = $state.snapshot(nextWorkspace);
      settings = $state.snapshot(nextSettings);
      status = 'Preferences saved';
    }}
    backup={async () => {
      if (await command('backup_library')) status = 'Backup exported';
    }}
    restore={async () => {
      if (metadataRunning || pending > 0)
        throw new Error(
          'Wait for background work to finish before restoring. Stop metadata fetching from the status bar if needed.',
        );
      previewWorkspace = null;
      modal = 'restore';
    }}
    folder={() =>
      task(async () => {
        await command('open_library_folder');
      })}
  />
{:else if modal === 'import'}<Modal
    {error}
    title="Bring your games together"
    close={() => {
      modal = null;
      apiKey = '';
    }}
    ><form
      onsubmit={(event) => {
        event.preventDefault();
        importGames();
      }}
    >
      <p class="muted">
        Import adds games without replacing your custom titles, favorites, or
        artwork. Your game files stay where they are.
      </p>
      <label
        >Library<select bind:value={importSource}
          ><option>Steam</option><option>Epic</option><option>Xbox</option
          ></select
        ></label
      ><label
        >{importSource === 'Epic' ? 'Manifest folder' : 'Library folder'}
        <div class="input-action">
          <input
            bind:value={importRoot}
            placeholder="Detect automatically"
          /><Button
            variant="secondary"
            type="button"
            onclick={() =>
              task(async () => {
                importRoot = (await pick('folder')) ?? importRoot;
              })}>Browse</Button
          >
        </div></label
      >{#if importSource === 'Steam'}<details>
          <summary>Include owned, uninstalled games</summary><label
            >SteamID64<input
              bind:value={accountId}
              inputmode="numeric"
              placeholder="Your numeric Steam account ID"
            /></label
          ><label
            >Steam Web API key<input
              type="password"
              bind:value={apiKey}
              autocomplete="off"
            /></label
          >
          <p class="field-hint">Used for this import only. Never saved.</p>
        </details>{/if}
      <div class="form-actions">
        <Button
          type="button"
          variant="secondary"
          onclick={() => {
            modal = null;
            apiKey = '';
          }}>Cancel</Button
        ><Button disabled={pending > 0}
          >{pending ? 'Importing…' : 'Import library'}</Button
        >
      </div>
    </form></Modal
  >
{:else if modal === 'metadata' && selected}<Modal
    {error}
    title="Find game information"
    close={() => (modal = null)}
    ><form
      onsubmit={(event) => {
        event.preventDefault();
        findMetadata();
      }}
    >
      <label
        >Search Steam
        <div class="input-action">
          <input bind:value={metadataQuery} /><Button
            variant="secondary"
            disabled={pending > 0}>Search</Button
          >
        </div></label
      >
    </form>
    {#if settings.artwork_provider === 'SteamGridDb'}<label
        >SteamGridDB session key<input
          type="password"
          bind:value={sessionArtKey}
          autocomplete="off"
        /></label
      >{/if}<label class="check-label"
      ><input type="checkbox" bind:checked={replaceMetadata} />Replace existing
      metadata and artwork</label
    >
    <div class="match-results">
      {#each matches as match}<button
          class="match-result"
          disabled={pending > 0}
          onclick={() => fetchMetadata(match.app_id)}
          ><span>{match.name}</span><span class="muted"
            >Steam #{match.app_id}</span
          ><Icon name="plus" size={16} /></button
        >{/each}
    </div>
    {#if selected.target.kind === 'steam' || selected.metadata.steam_app_id}<Button
        disabled={pending > 0}
        onclick={() => fetchMetadata()}>Fetch linked game information</Button
      >{/if}</Modal
  >
{:else if modal === 'artwork' && selected}<Modal
    {error}
    title="Give it a new cover"
    close={() => (modal = null)}
    ><div class="backup-actions">
      <Button variant="secondary" onclick={chooseLocalCover}
        ><Icon name="folder" />Choose a local cover</Button
      ><Button
        variant="secondary"
        onclick={() =>
          task(async () => {
            if (!selected) return;
            await updateGame(selected, { cover: null });
            modal = null;
          })}>Use generated artwork</Button
      >
    </div>
    <hr />
    <label
      >Browse artwork<select
        bind:value={imageKind}
        onchange={() => (imageChoices = [])}
        ><option value="cover">Covers</option><option value="background"
          >Backgrounds</option
        ><option value="logo">Logos</option></select
      ></label
    >{#if settings.artwork_provider === 'SteamGridDb'}<label
        >SteamGridDB session key<input
          type="password"
          bind:value={sessionArtKey}
          autocomplete="off"
        /></label
      >{/if}<Button
      variant="secondary"
      disabled={pending > 0}
      onclick={browseArtwork}>Find artwork</Button
    >
    <div class="artwork-results">
      {#each imageChoices as choice}<ArtworkChoice
          {choice}
          disabled={pending > 0}
          select={() => useArtwork(choice)}
        />{/each}
    </div>
    <p class="field-hint">
      Link this game to Steam in Find metadata if no artwork is available.
    </p></Modal
  >
{:else if modal === 'remove'}<Modal
    {error}
    title="Remove this game?"
    close={() => (modal = null)}
    ><p>
      Remove {selected?.name} from your collection? Its files will stay on your device.
    </p>
    <div class="form-actions">
      <Button variant="secondary" onclick={() => (modal = null)}>Keep game</Button
      ><Button disabled={pending > 0} onclick={removeGame}
        >Remove game</Button
      >
    </div></Modal
  >
{:else if modal === 'restore'}<Modal
    {error}
    title="Restore your library?"
    close={() => (modal = null)}
    ><p>
      Choose a library backup to replace your current collection. Export a
      backup first if you want to keep the current entries.
    </p>
    <div class="form-actions">
      <Button variant="secondary" onclick={() => (modal = null)}>Cancel</Button
      ><Button
        disabled={pending > 0}
        onclick={() =>
          task(async () => {
            if (!(await command('restore_library'))) return;
            await reload();
            selectedKey = null;
            modal = null;
            status = 'Library restored';
          })}>Choose backup</Button
      >
    </div></Modal
  >{/if}
