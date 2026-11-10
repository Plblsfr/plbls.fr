<script lang="ts">
  import Loader from '../../components/Loader.svelte'
  import { onDestroy } from 'svelte'
  import { app } from '../../lib/store.svelte'
  import { voice } from './voice.svelte'
  import { buildItems, gridDims, type StageItem } from './stage'
  import type { Channel, SessionKind } from '../../lib/types'
  import Avatar from '../../components/Avatar.svelte'
  import { openProfile } from '../../lib/profile.svelte'
  import Icon from '../../components/Icon.svelte'
  import WatchTogether from '../regarder-ensemble/WatchTogether.svelte'
  import FilmNoAccess from '../../sections/entracte/FilmNoAccess.svelte'
  import Draw2Gether from '../draw2gether/Draw2Gether.svelte'
  import { draw } from '../draw2gether/draw.svelte'
  import NowPlaying from '../../components/NowPlaying.svelte'
  import Modal from '../../components/Modal.svelte'
  import { api } from '../../lib/api'
  import { notify } from '../../lib/ui'
  import StageTile from './StageTile.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import VoiceControls from './VoiceControls.svelte'

  let { channel, onmenu }: { channel: Channel; onmenu: () => void } = $props()

  const serverId = $derived(app.channel(channel.id)?.server.id)
  const here = $derived(voice.channelId === channel.id && voice.state !== 'idle')
  const connecting = $derived(voice.channelId === channel.id && voice.state === 'connecting')
  const people = $derived.by(() => {
    const list = (app.voice[channel.id] ?? []).filter((p) => !p.user_id.endsWith('#screen')) // participant du partage natif : pas une personne en plus
    // soi-même est affiché dès la connexion, avant que le serveur ne l'ait ajouté à la liste
    return here && !list.some((p) => p.user_id === app.meId) ? [{ user_id: app.meId, pseudo: app.pseudo }, ...list] : list
  })
  const items = $derived(buildItems(people, here ? voice.tiles : []))

  // « Regarder ensemble » (EST-143) et Music2gether (EST-168) : deux sessions distinctes ; la session affichée prend la scène, les participants passent dans la bande.
  // Salon cinéma : la seule session est la séance de film (`cinema`), sans Music2gether ni Draw2Gether.
  const cinema = $derived(channel.kind === 'cinema')
  const watchKind = $derived<SessionKind>(cinema ? 'cinema' : 'watch')
  const watching = $derived(here && !!app[watchKind][channel.id])
  const listening = $derived(here && !cinema && !!app.music[channel.id])
  const game = $derived(app.on('draw2gether') && !cinema ? draw.games[channel.id] : undefined)
  const playing = $derived(here && !!game && !game.hidden)
  $effect(() => { if (here && !cinema && app.on('draw2gether')) void draw.load(channel.id) })
  async function startGame() {
    if (game) game.hidden = false
    try { await api.drawJoin(channel.id) } catch (err) { notify.error(err, 'Impossible de lancer la partie') }
  }
  let pick = $state<SessionKind>('music') // vue choisie quand les deux sessions existent
  const view = $derived<SessionKind>(watching && listening ? pick : listening ? 'music' : 'watch')
  let startOpen = $state<SessionKind | null>(null)
  let startLink = $state('')
  async function startSession(e: Event) {
    e.preventDefault()
    const kind = startOpen!
    try { await api.watchStart(channel.id, startLink.trim(), undefined, kind); if (kind === 'music') pick = 'music'; startOpen = null; startLink = '' } catch (err) { notify.error(err, 'Impossible de lancer la session') }
  }

  // Un partage d'écran passe automatiquement au premier plan ; « Détacher » le remet dans la grille.
  let dismissed = $state<string | null>(null)
  const focus = $derived<StageItem | undefined>(
    items.find((i) => i.tile && i.tile.sid === voice.pinned) ?? items.find((i) => i.tile?.screen && i.tile.track && i.key !== dismissed),
  )
  const strip = $derived(focus ? items.filter((i) => i.key !== focus.key) : [])
  $effect(() => { if (dismissed && !items.some((i) => i.key === dismissed)) dismissed = null })

  function pin(i: StageItem) {
    if (focus?.key === i.key) {
      if (voice.pinned === i.tile?.sid) voice.pinned = null
      else dismissed = i.key
    } else {
      voice.pinned = i.tile?.sid ?? null
      dismissed = null
    }
  }

  const watchers = $derived(voice.watchers[items.find((i) => i.userId === app.meId && i.tile?.screen)?.tile?.sid ?? '']?.length ?? 0)

  const dims = $derived(gridDims(items.length))
  const dimsNarrow = $derived(gridDims(items.length, true))

  // Mode plein cadre : la barre latérale et l'en-tête disparaissent, les commandes passent en barre flottante.
  let theater = $state(false)
  $effect(() => {
    document.documentElement.toggleAttribute('data-theater', theater && here)
    if (theater && here) void document.documentElement.requestFullscreen?.().catch(() => {})
    else if (document.fullscreenElement) void document.exitFullscreen().catch(() => {})
  })
  $effect(() => {
    const exit = () => { if (!document.fullscreenElement) theater = false }
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') theater = false }
    document.addEventListener('fullscreenchange', exit)
    addEventListener('keydown', esc)
    return () => { document.removeEventListener('fullscreenchange', exit); removeEventListener('keydown', esc) }
  })
  onDestroy(() => document.documentElement.removeAttribute('data-theater'))
</script>

<header class="main-head vc-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1 class="grow row"><span class="vc-head-ic"><Icon name={cinema ? 'film' : 'vol'} size={20} /></span> {channel.name}</h1>
  {#if here}
    <span class="v-mono v-mute vc-count">{people.length} / 10</span>
    <span class="vc-acts">
      {#if watching && listening}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (pick = view === 'music' ? 'watch' : 'music')} aria-label={view === 'music' ? 'Afficher la vidéo' : 'Afficher la musique'} title={view === 'music' ? 'Afficher la vidéo' : 'Afficher la musique'}><Icon name={view === 'music' ? 'tele' : 'note'} size={16} /><span class="vc-txt">{view === 'music' ? 'Vidéo' : 'Musique'}</span></button>{/if}
      {#if cinema}
        {#if !watching && app.movies}<a class="plbls-button plbls-button--compact plbls-button--contour" href={hrefs.films()} aria-label="Choisir un film" title="Choisir un film dans la bibliothèque"><Icon name="film" size={16} /><span class="vc-txt">Choisir un film</span></a>{/if}
      {:else}
        {#if !watching}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (startOpen = 'watch')} aria-label="Regarder ensemble" title="Regarder ensemble (vidéo)"><Icon name="tele" size={16} /><span class="vc-txt">Regarder ensemble</span></button>{/if}
        {#if !listening}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (startOpen = 'music')} aria-label="Écouter ensemble" title="Écouter ensemble (Music2gether)"><Icon name="note" size={16} /><span class="vc-txt">Écouter ensemble</span></button>{/if}
        {#if app.on('draw2gether') && app.peut('draw2gether.use') && !playing}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void startGame()} aria-label="Draw2Gether" title="Draw2Gether (dessiner et deviner)"><Icon name="pinceau" size={16} /><span class="vc-txt">Draw2Gether</span></button>{/if}
      {/if}
      <button class="ib" aria-pressed={theater} onclick={() => (theater = !theater)} aria-label="Mode plein cadre" title="Mode plein cadre"><Icon name={theater ? 'reduire' : 'agrandir'} /></button>
    </span>
  {/if}
</header>

<div class="stage" class:focus={!!focus || watching || listening || playing}>
  {#if here && !connecting}
    {#if playing || watching || listening}
      <div class="stage-main">{#if playing}<Draw2Gether channelId={channel.id} serverId={serverId ?? ""} />{:else if view === 'music'}<NowPlaying channelId={channel.id} {serverId} {people} />{:else if cinema && !app.movies}<FilmNoAccess channelId={channel.id} roomName={channel.name} {people} />{:else}<WatchTogether channelId={channel.id} {people} kind={watchKind} {theater} ontheater={() => (theater = !theater)} />{/if}</div>
      <ul class="participants stage-strip">
        {#each items as it (it.key)}<StageTile item={it} {serverId} self={it.userId === app.meId} onpin={() => pin(it)} />{/each}
      </ul>
    {:else if focus}
      <ul class="participants stage-main">
        <StageTile item={focus} {serverId} self={focus.userId === app.meId} big onpin={() => pin(focus)} />
      </ul>
      {#if strip.length}
        <ul class="participants stage-strip">
          {#each strip as it (it.key)}<StageTile item={it} {serverId} self={it.userId === app.meId} onpin={() => pin(it)} />{/each}
        </ul>
      {/if}
    {:else}
      <ul class="participants stage-grid" class:solo={items.length === 1} style="--cols: {dims.cols}; --rows: {dims.rows}; --cols-n: {dimsNarrow.cols}; --rows-n: {dimsNarrow.rows}">
        {#each items as it (it.key)}<StageTile item={it} {serverId} self={it.userId === app.meId} onpin={() => pin(it)} />{/each}
      </ul>
    {/if}
    {#if cinema && !watching}
      <p class="stage-hint v-note">Aucun film en cours.{#if app.movies} <a href={hrefs.films()}>Choisir un film dans la bibliothèque</a> : il sera lu ici pour tous, piloté par toi.{/if}</p>
    {/if}
    {#if people.length === 1}<p class="stage-hint v-note">Tu es seul·e ici. Invite quelqu'un : tout le monde apparaîtra sur cette scène.</p>{/if}
    {#if voice.screen}
      <div class="share-banner" role="status">
        <span class="share-banner-ic"><Icon name="ecran" size={16} /></span>
        <strong>Tu partages ton écran</strong>
        <span class="v-mono share-banner-n">{watchers} spectateur{watchers > 1 ? 's' : ''}</span>
        <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void voice.restartScreen()}>Changer de source</button>
        <button class="plbls-button plbls-button--compact plbls-button--danger" onclick={() => voice.requestScreen()}>Arrêter</button>
      </div>
    {/if}
    {#if !(theater && watching && view === 'watch' && !playing)}<div class="theater-bar">
      <VoiceControls />
      {#if theater}<button class="theater-exit" onclick={() => (theater = false)} aria-label="Quitter le mode plein cadre" title="Quitter le mode plein cadre (Échap)"><Icon name="reduire" size={20} /></button>{/if}
    </div>{/if}
  {:else}
    <div class="stage-join">
      {#if people.length}
        <ul class="join-faces" aria-label="Participants">
          {#each people as p (p.user_id)}<li><button class="profile-link" onclick={() => openProfile(p.user_id, serverId)} title="Profil de {p.pseudo}"><Avatar pseudo={p.pseudo} userId={p.user_id} {serverId} size="lg" speaking={!!voice.speaking[p.user_id]} /><span class="join-name">{p.pseudo}</span></button></li>{/each}
        </ul>
      {/if}
      <div class="join-title">
        <h2>{channel.name}</h2>
        <p class="v-mute">{people.length ? `${people.length} / 10 participant${people.length > 1 ? 's' : ''}` : 'Personne pour le moment.'}</p>
      </div>
      {#if voice.error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{voice.error}</p></div>{/if}
      {#if connecting}
        <Loader label="Connexion au vocal" />
      {:else if app.can(serverId, 'connect', channel.id)}
        <button class="plbls-button v-rose join-go" onclick={() => voice.join(channel.id)}><Icon name={cinema ? 'film' : 'vol'} /> Rejoindre le salon</button>
      {:else}
        <p class="v-alert att join-deny"><Icon name="cadenas" /> Tu n'as pas la permission de rejoindre ce salon vocal.</p>
      {/if}
      <p class="v-note">L'audio passe par le serveur vocal. Le navigateur te demandera l'accès au micro, puis à la caméra ou à l'écran si tu les actives.</p>
    </div>
  {/if}
</div>

{#if startOpen}
  <Modal title={startOpen === 'music' ? 'Écouter ensemble' : 'Regarder ensemble'} onclose={() => (startOpen = null)}>
    <form class="vc-start" onsubmit={startSession}>
      {#if startOpen === 'music'}
        <div class="plbls-field">
          <label class="plbls-field__label" for="vc-lien">Lien de la musique</label>
          <input id="vc-lien" class="plbls-input" bind:value={startLink} maxlength="2000" placeholder="YouTube Music : morceau, album ou playlist" aria-label="Lien de la musique" />
          <p class="plbls-field__aide">Chaque participant joue la musique avec le lecteur officiel de YouTube ; Estompe ne fait que synchroniser la lecture. Cette session est indépendante de « Regarder ensemble ».</p>
        </div>
      {:else}
        <div class="plbls-field">
          <label class="plbls-field__label" for="vc-lien">Lien de la vidéo</label>
          <input id="vc-lien" class="plbls-input" bind:value={startLink} maxlength="2000" placeholder="YouTube, Twitch, Vimeo, Dailymotion ou fichier" aria-label="Lien de la vidéo" />
          <p class="plbls-field__aide">Chaque participant lit la vidéo avec le lecteur de sa source ; Estompe ne fait que synchroniser la lecture. Les directs et clips Twitch se lancent en même temps, sans pause ni déplacement partagés.</p>
        </div>
      {/if}
      <div class="vc-start-actions">
        <button type="button" class="plbls-button plbls-button--contour" onclick={() => (startOpen = null)}>Annuler</button>
        <button class="plbls-button v-rose" disabled={!startLink.trim()}>Lancer</button>
      </div>
    </form>
  </Modal>
{/if}

<style>
  .vc-head-ic { display: inline-flex; color: var(--gris); }
  .vc-count { flex: none; }
  .vc-acts { display: inline-flex; align-items: center; gap: 6px; flex-wrap: nowrap; min-width: 0; }
  .vc-acts .plbls-button { white-space: nowrap; text-decoration: none; }
  @media (max-width: 1100px) { .vc-txt { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; } .vc-acts .plbls-button { padding-inline: 8px; } }
  @media (pointer: coarse) { .vc-acts .plbls-button { min-height: 44px; min-width: 44px; } }
  .stage-join { place-self: center; width: min(520px, 100%); display: flex; flex-direction: column; align-items: center; gap: 20px; text-align: center; }
  .join-title { display: grid; gap: 6px; }
  .join-title h2 { margin: 0; font-family: var(--f-titre); font-size: clamp(32px, 6vw, 44px); font-weight: 700; letter-spacing: -0.04em; line-height: 1.05; overflow-wrap: anywhere; }
  .join-title p { margin: 0; }
  .stage-join .plbls-alert, .join-deny { width: 100%; text-align: left; margin: 0; }
  .join-deny { align-items: center; }
  .join-go { min-height: 48px; padding-inline: 24px; font-size: 15px; }
  .join-faces { display: flex; flex-wrap: wrap; justify-content: center; gap: 12px; margin: 0; padding: 0; list-style: none; }
  .join-faces li { display: grid; justify-items: center; }
  .join-faces .profile-link { flex-direction: column; gap: 6px; padding: 4px; border-radius: var(--r); }
  .join-faces .profile-link:focus-visible { outline: none; box-shadow: var(--anneau); }
  .join-name { font-size: 13px; color: var(--encre-2); max-width: 96px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vc-start { display: grid; gap: 16px; min-width: min(420px, 80vw); }
  .vc-start-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
