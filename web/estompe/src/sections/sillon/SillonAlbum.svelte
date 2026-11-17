<script lang="ts">
  import { ApiError } from '../../lib/api'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import SillonCover from './SillonCover.svelte'
  import SillonTracks from './SillonTracks.svelte'
  import { menuFrom } from '../../lib/ctxmenu.svelte'
  import SillonLike from './SillonLike.svelte'
  import { copierLien, echec, lecteur, menuContenu, type Contexte } from './sillon.svelte'
  import { anneeEtType, sillonApi, titresDe, totalAlbum, type FicheAlbum } from './sillon'

  let { id, focus }: { id: string; focus?: string } = $props()
  let plus = $state<HTMLButtonElement>()

  let fiche = $state<FicheAlbum | null>(null)
  let manque = $state(false)
  const contexte = $derived<Contexte>({ type: 'album', id })
  const ici = $derived(lecteur.contexte.type === 'album' && lecteur.contexte.id === id && !!lecteur.piste)
  const lisibles = $derived(fiche?.tracks.filter((t) => t.available).length ?? 0)

  $effect(() => {
    let gone = false
    fiche = null
    manque = false
    sillonApi.album(id).then((f) => { if (!gone) fiche = f }).catch((e) => { if (gone) return; manque = true; if (!(e instanceof ApiError && e.status === 404)) echec(e, 'Cet album est indisponible') })
    return () => { gone = true }
  })
</script>

{#if fiche}
  <div class="film-page film-page--flush">
    <div class="film-hero sillon-hero">
      <a class="film-back plbls-button plbls-button--compact plbls-button--contour" href={hrefs.musique('artist', fiche.artist.id)}><Icon name="gauche" size={14} /> {fiche.artist.name}</a>
      <div class="film-hero-body">
        <span class="film-card-art film-body-poster"><SillonCover id={fiche.id} title={fiche.title} image={fiche.cover} size="large" /></span>
        <div class="film-info">
          <p class="v-eyebrow">{[anneeEtType(fiche), titresDe(lisibles), totalAlbum(fiche.tracks)].filter(Boolean).join(' · ')}</p>
          <h1>{fiche.title}</h1>
          <p class="film-overview"><a class="sillon-lien" href={hrefs.musique('artist', fiche.artist.id)}>{fiche.artist.name}</a>{#if fiche.genres.length}{' · '}{fiche.genres.slice(0, 3).join(', ')}{/if}</p>
          <div class="film-actions">
            {#if ici}
              <button class="plbls-button" onclick={() => lecteur.basculer()}><Icon name={lecteur.lecture ? 'pause' : 'lecture'} size={18} /> {lecteur.lecture ? 'Pause' : 'Reprendre'}</button>
            {:else}
              <button class="plbls-button" disabled={!lisibles} onclick={() => fiche && lecteur.jouer(fiche.tracks, 0, contexte)}><Icon name="lecture" size={18} /> Écouter l’album</button>
            {/if}
            <SillonLike kind="albums" id={fiche.id} titre={fiche.title} grand />
            <button class="v-ib sq" bind:this={plus} aria-label="Plus d’actions sur l’album" title="Plus" onclick={() => fiche && plus && menuFrom(plus, [...(lisibles ? menuContenu({ album: fiche.id }, fiche.title) : []), { label: 'Copier le lien', run: () => fiche && copierLien('album', fiche.id), sep: !!lisibles }])}><Icon name="plus3" size={18} /></button>
          </div>
          {#if !fiche.available}<p class="v-note"><span class="v-tag">Indisponible</span> Les fichiers de cet album ne sont plus sur le serveur.</p>{/if}
        </div>
      </div>
    </div>
    <section class="film-body">
      <SillonTracks pistes={fiche.tracks} {contexte} album {focus} />
    </section>
  </div>
{:else if manque}
  <div class="film-page"><div class="film-missing"><Icon name="alerte" /><span class="grow">Cet album est indisponible.</span><a class="plbls-button plbls-button--compact plbls-button--contour" href={hrefs.musique()}>Retour à l’accueil</a></div></div>
{:else}
  <div class="film-page"><Loader /></div>
{/if}

<style>
  .sillon-lien { color: inherit; font-weight: 600; text-decoration: underline; text-decoration-color: var(--rose-fonce); text-underline-offset: 3px; }
</style>
