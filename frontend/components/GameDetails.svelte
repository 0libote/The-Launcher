<script lang="ts">
  import type { Game } from '../lib/model';
  import { artSeed, initials, lastPlayed, source } from '../lib/model';
  import ArtworkImage from './ArtworkImage.svelte';
  import Cover from './Cover.svelte';
  import Button from './ui/button.svelte';
  import Icon from './Icon.svelte';
  let {
    game,
    cover,
    close,
    play,
    favorite,
    edit,
    artwork,
    metadata,
    remove,
    hidden,
    busy,
    openLink,
  }: {
    game: Game;
    cover: string | null;
    close: () => void;
    play: () => void;
    favorite: () => void;
    edit: () => void;
    artwork: () => void;
    metadata: () => void;
    remove: () => void;
    hidden: () => void;
    busy: boolean;
    openLink: (index: number) => void;
  } = $props();
  const seed = $derived(artSeed(game.name));
  const playLabel = $derived(
    game.installed === false && game.target.kind === 'steam'
      ? 'Install with Steam'
      : 'Play game',
  );
</script>

<section class="game-details" aria-label={`${game.name} details`}>
  <div class="detail-hero art-{seed}">
    {#if game.metadata.background}<ArtworkImage {game} kind="background" />{/if}
    <div class="detail-hero-fallback" aria-hidden="true">
      <span>{initials(game.name)}</span>
    </div>
    <div class="detail-hero-scrim" aria-hidden="true"></div>
    <span class="detail-hero-source">{source(game)}</span>
    <Button
      variant="icon"
      class="detail-hero-close"
      onclick={close}
      aria-label="Close game details"><Icon name="close" /></Button
    >
  </div>
  {#if game.metadata.logo}<div class="detail-logo">
      <ArtworkImage {game} kind="logo" />
    </div>{/if}
  <div class="detail-identity">
    <div class="detail-cover"><Cover {game} url={cover} /></div>
    <div class="detail-heading">
      <div class="detail-title">
        <h2>{game.name}</h2>
        <Button
          variant="icon"
          class={game.favorite ? 'is-favorite' : ''}
          onclick={favorite}
          aria-label={game.favorite
            ? 'Remove from favorites'
            : 'Add to favorites'}
          aria-pressed={game.favorite}><Icon name="heart" /></Button
        >
      </div>
      <p class="detail-subtitle">
        {game.metadata.developers.join(', ') || source(game)}{game.metadata
          .release_date
          ? ` / ${game.metadata.release_date}`
          : ''}
      </p>
      {#if game.metadata.genres.length}<div class="tags">
          {#each game.metadata.genres as genre}<span>{genre}</span>{/each}
        </div>{/if}
    </div>
  </div>
  <Button class="play-button" disabled={busy} onclick={play}
    ><Icon
      name={game.installed === false && game.target.kind === 'steam'
        ? 'download'
        : 'play'}
    />{playLabel}</Button
  >
  <div class="detail-columns">
    <div class="detail-main">
      <h3 class="detail-section">About</h3>
      {#if game.metadata.description}<p class="description">
          {game.metadata.description}
        </p>{:else}<p class="description muted">
          Add a description and artwork by finding this game’s metadata.
        </p>{/if}
      {#if game.metadata.tags.length}<div class="tags">
          {#each game.metadata.tags as tag}<span>{tag}</span>{/each}
        </div>{/if}
    </div>
    <div class="detail-side">
      <h3 class="detail-section">Information</h3>
      <dl class="detail-stats">
        <div>
          <dt>Last played</dt>
          <dd>{lastPlayed(game.last_launched)}</dd>
        </div>
        <div>
          <dt>Availability</dt>
          <dd>
            {game.installed === true
              ? 'Installed'
              : game.installed === false
                ? 'Not installed'
                : 'Not checked'}
          </dd>
        </div>
        {#if game.metadata.critic_score !== null}<div>
            <dt>Critic score</dt>
            <dd>{game.metadata.critic_score}/100</dd>
          </div>{/if}
        {#if game.metadata.series}<div>
            <dt>Series</dt>
            <dd>{game.metadata.series}</dd>
          </div>{/if}
      </dl>
      <dl class="metadata-facts">
        {#if game.metadata.publishers.length}<div>
            <dt>Publisher</dt>
            <dd>{game.metadata.publishers.join(', ')}</dd>
          </div>{/if}
        {#if game.metadata.features.length}<div>
            <dt>Features</dt>
            <dd>{game.metadata.features.join(', ')}</dd>
          </div>{/if}
        {#if game.metadata.age_rating}<div>
            <dt>Age rating</dt>
            <dd>{game.metadata.age_rating}</dd>
          </div>{/if}
      </dl>
      {#each game.metadata.links as link, index}<Button
          variant="ghost"
          class="store-link"
          onclick={() => openLink(index)}>Open store page</Button
        >{/each}
      {#if Object.keys(game.metadata.sources).length}<details
          class="attribution"
        >
          <summary>Metadata sources</summary
          >{#each Object.entries(game.metadata.sources) as [field, provider]}<p>
              {field.replaceAll('_', ' ')}: {provider}
            </p>{/each}
        </details>{/if}
    </div>
  </div>
  <h3 class="detail-section">Manage</h3>
  <div class="detail-actions">
    <Button variant="secondary" onclick={edit}
      ><Icon name="edit" />Edit game</Button
    ><Button variant="secondary" onclick={artwork}
      ><Icon name="image" />Artwork</Button
    ><Button variant="secondary" onclick={metadata}
      ><Icon name="refresh" />Find metadata</Button
    ><Button variant="ghost" onclick={hidden}
      ><Icon name="eye" />{game.hidden
        ? 'Restore to library'
        : 'Hide from library'}</Button
    >{#if game.target.kind === 'executable'}<Button
        variant="danger"
        onclick={remove}>Remove from library</Button
      >{/if}
  </div>
</section>
