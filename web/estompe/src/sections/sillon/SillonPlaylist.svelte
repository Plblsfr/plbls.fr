<script lang="ts">
  import { ApiError } from '../../lib/api'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs, router } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import SillonMosaique from './SillonMosaique.svelte'
  import SillonTracks from './SillonTracks.svelte'
  import { collection, copierLien, echec, lecteur, type Contexte } from './sillon.svelte'
  import { deplacerListe, dureeTotale, sillonApi, titresDe, type FichePlaylist, type Piste } from './sillon'

  let { id }: { id?: string } = $props()

  type Vue = { nom: string; albums: string[]; duree: number | null; pistes: Piste[]; cles: string[] }
  let fiche = $state<FichePlaylist | null>(null)
  let vue = $state<Vue | null>(null)
  let manque = $state(false)
  const contexte = $derived<Contexte>(id ? { type: 'playlist', id } : { type: 'likes' })
  const lisibles = $derived(vue?.pistes.filter((p) => p.available).length ?? 0)
  const ici = $derived(!!lecteur.piste && JSON.stringify(lecteur.contexte) === JSON.stringify(contexte))

  const depuis = (f: FichePlaylist): Vue => ({ nom: f.name, albums: f.cover_albums, duree: f.duration, pistes: f.items.map((e) => e.track), cles: f.items.map((e) => e.id) })

  $effect(() => {
    let gone = false
    fiche = null
    vue = null
    manque = false
    const charge = id
      ? sillonApi.playlist(id).then((f) => { if (!gone) { fiche = f; vue = depuis(f) } })
      : sillonApi.titresLikes().then((l) => { if (!gone) vue = { nom: 'Titres likés', albums: l.cover_albums, duree: l.duration, pistes: l.items.map((e) => e.track), cles: l.items.map((e) => e.track.id) } })
    charge.catch((e) => { if (gone) return; manque = true; if (!(e instanceof ApiError && e.status === 404)) echec(e, 'Cette playlist est indisponible') })
    return () => { gone = true }
  })

  function jouer(melanger: boolean) {
    if (!vue) return
    const dispo = vue.pistes.map((p, i) => (p.available ? i : -1)).filter((i) => i >= 0)
    lecteur.jouer(vue.pistes, melanger ? dispo[Math.floor(Math.random() * dispo.length)] : 0, contexte, melanger)
  }

  async function deplacer(from: number, to: number) {
    if (!fiche || !id) return
    const avant = fiche
    fiche = { ...fiche, items: deplacerListe(fiche.items, from, to) }
    vue = depuis(fiche)
    try {
      fiche = await sillonApi.deplacerDansPlaylist(id, from, to)
      vue = depuis(fiche)
    } catch (e) {
      fiche = avant
      vue = depuis(avant)
      notify.error(e, 'L’ordre n’a pas pu être enregistré')
    }
  }

  async function retirer(i: number) {
    if (!fiche || !id) return
    try {
      fiche = await sillonApi.retirerDePlaylist(id, fiche.items[i].id)
      vue = depuis(fiche)
      collection.maj(fiche)
    } catch (e) { notify.error(e, 'Le titre n’a pas pu être retiré') }
  }

  async function renommer() {
    if (!fiche) return
    const r = await collection.renommer(fiche)
    if (r) { fiche = r; vue = depuis(r) }
  }

  async function supprimer() {
    if (fiche && (await collection.supprimer(fiche))) router.go(hrefs.musique())
  }
</script>

{#if vue}
  <div class="film-page film-page--flush">
    <div class="film-hero sillon-hero">
      <div class="film-hero-body">
        <span class="film-card-art film-body-poster">
          {#if id}<SillonMosaique albums={vue.albums} {id} titre={vue.nom} size="large" />{:else}<span class="sillon-likes-art"><Icon name="coeurplein" size={72} /></span>{/if}
        </span>
        <div class="film-info">
          <p class="v-eyebrow">{['Playlist', titresDe(vue.pistes.length), dureeTotale(vue.duree)].join(' · ')}</p>
          <h1>{vue.nom}</h1>
          {#if fiche?.description}<p class="film-overview">{fiche.description}</p>{/if}
          <div class="film-actions">
            {#if ici}
              <button class="plbls-button" onclick={() => lecteur.basculer()}><Icon name={lecteur.lecture ? 'pause' : 'lecture'} size={18} /> {lecteur.lecture ? 'Pause' : 'Reprendre'}</button>
            {:else}
              <button class="plbls-button" disabled={!lisibles} onclick={() => jouer(false)}><Icon name="lecture" size={18} /> Écouter</button>
            {/if}
            <button class="plbls-button plbls-button--contour" disabled={!lisibles} onclick={() => jouer(true)}><Icon name="shuffle" size={18} /> Aléatoire</button>
            {#if fiche}
              <button class="v-ib sq" onclick={() => void renommer()} aria-label="Renommer la playlist" title="Renommer"><Icon name="crayon" size={18} /></button>
              <button class="v-ib sq" onclick={() => fiche && copierLien('playlist', fiche.id)} aria-label="Copier le lien de la playlist" title="Copier le lien"><Icon name="lien" size={18} /></button>
              <button class="v-ib sq" onclick={() => void supprimer()} aria-label="Supprimer la playlist" title="Supprimer"><Icon name="corbeille" size={18} /></button>
            {/if}
          </div>
        </div>
      </div>
    </div>
    <section class="film-body">
      {#if vue.pistes.length}
        {#if fiche}<p class="v-note sillon-aide">Glisse un titre ou utilise Alt + ↑ / ↓ pour changer l’ordre.</p>{/if}
        <SillonTracks pistes={vue.pistes} cles={vue.cles} {contexte} deplacer={fiche ? (a, b) => void deplacer(a, b) : undefined}
          extra={fiche ? (_p, i) => [{ label: 'Retirer de la playlist', run: () => void retirer(i), danger: true }] : undefined} />
      {:else}
        <div class="v-empty"><Icon name={id ? 'note' : 'coeur'} size={28} /><p class="v-note">{id ? 'Cette playlist est vide. Ajoute des titres depuis le menu d’un titre ou d’un album.' : 'Aucun titre liké pour l’instant.'}</p></div>
      {/if}
    </section>
  </div>
{:else if manque}
  <div class="film-page"><div class="film-missing"><Icon name="alerte" /><span class="grow">Cette playlist est indisponible.</span><a class="plbls-button plbls-button--compact plbls-button--contour" href={hrefs.musique()}>Retour à l’accueil</a></div></div>
{:else}
  <div class="film-page"><Loader /></div>
{/if}

<style>
  .sillon-likes-art { position: absolute; inset: 0; display: grid; place-items: center; background: var(--rose); color: var(--rose-fonce); }
  .sillon-aide { margin: 0 0 8px 10px; }
</style>
