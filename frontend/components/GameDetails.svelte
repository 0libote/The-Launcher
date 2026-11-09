<script lang="ts">
  import type { Game } from '../lib/model';
  import { lastPlayed, source } from '../lib/model';
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
</script>

<section class="game-details" aria-label={`${game.name} details`}>
  {#if game.metadata.background}<div class="detail-hero">
      <ArtworkImage {game} kind="background" />
    </div>{/if}
  <div class="detail-top">
    <span class="source-label">{source(game)}</span><Button
      variant="icon"
      onclick={close}
      aria-label="Close game details"><Icon name="close" /></Button
    >
  </div>
  <div class="detail-cover"><Cover {game} url={cover} /></div>
  {#if game.metadata.logo}<div class="detail-logo">
      <ArtworkImage {game} kind="logo" />
    </div>{/if}
  <div class="detail-title">
    <h2>{game.name}</h2>
    <Button
      variant="icon"
      class={game.favorite ? 'is-favorite' : ''}
      onclick={favorite}
      aria-label={game.favorite ? 'Remove from favorites' : 'Add to favorites'}
      aria-pressed={game.favorite}><Icon name="heart" /></Button
    >
  </div>
  <p class="detail-subtitle">
    {game.metadata.developers.join(', ') || source(game)}{game.metadata
      .release_date
      ? ` / ${game.metadata.release_date}`
      : ''}
  </p>
  <Button class="play-button" disabled={busy} onclick={play}
    ><Icon
      name={game.installed === false && game.target.kind === 'steam'
        ? 'download'
        : 'play'}
    />{game.installed === false && game.target.kind === 'steam'
      ? 'Install with Steam'
      : 'Play game'}</Button
  >
  <div class="detail-facts">
    <div>
      <span>Last played</span><strong>{lastPlayed(game.last_launched)}</strong>
    </div>
    <div>
      <span>Availability</span><strong
        >{game.installed === true
          ? 'Installed'
          : game.installed === false
            ? 'Not installed'
            : 'Not checked'}</strong
      >
    </div>
  </div>
  {#if game.metadata.genres.length}<div class="tags">
      {#each game.metadata.genres as genre}<span>{genre}</span>{/each}
    </div>{/if}
  {#if game.metadata.description}<p class="description">
      {game.metadata.description}
    </p>{:else}<p class="description muted">
      Add a description and artwork by finding this game’s metadata.
    </p>{/if}
  {#if game.metadata.tags.length}<div class="tags">
      {#each game.metadata.tags as tag}<span>{tag}</span>{/each}
    </div>{/if}
  {#if game.metadata.critic_score !== null}<p class="muted">
      Critic score: {game.metadata.critic_score}/100
    </p>{/if}
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
    {#if game.metadata.series}<div>
        <dt>Series</dt>
        <dd>{game.metadata.series}</dd>
      </div>{/if}
  </dl>
  {#each game.metadata.links as link, index}<Button
      variant="ghost"
      class="store-link"
      onclick={() => openLink(index)}>Open store page</Button
    >{/each}
  {#if Object.keys(game.metadata.sources).length}<details class="attribution">
      <summary>Metadata sources</summary
      >{#each Object.entries(game.metadata.sources) as [field, provider]}<p>
          {field.replaceAll('_', ' ')}: {provider}
        </p>{/each}
    </details>{/if}
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
