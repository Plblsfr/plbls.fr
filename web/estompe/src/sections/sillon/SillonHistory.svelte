<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import SillonCover from './SillonCover.svelte'
  import SillonTracks from './SillonTracks.svelte'
  import { echec, lecteur } from './sillon.svelte'
  import { sillonApi, type Artiste, type Ecoute, type Periode, type Piste } from './sillon'

  type Onglet = 'recent' | 'tracks' | 'artists'
  const ONGLETS: { id: Onglet; label: string }[] = [{ id: 'recent', label: 'Écoutes récentes' }, { id: 'tracks', label: 'Titres' }, { id: 'artists', label: 'Artistes' }]
  const PERIODES: { id: Periode; label: string }[] = [{ id: 'week', label: '7 jours' }, { id: 'month', label: '30 jours' }, { id: 'year', label: '1 an' }, { id: 'all', label: 'Toujours' }]

  let onglet = $state<Onglet>('recent')
  let periode = $state<Periode>('month')
  let ecoutes = $state<Ecoute[]>([])
  let suite = $state<string | null>(null)
  let pistes = $state<{ track: Piste; plays: number }[]>([])
  let artistes = $state<{ artist: Artiste; plays: number }[]>([])
  let loading = $state(true)
  const fois = (n: number) => `${n} écoute${n > 1 ? 's' : ''}`

  async function plus(cursor: string | null) {
    loading = true
    try {
      const r = await sillonApi.ecoutes(cursor)
      ecoutes = cursor ? [...ecoutes, ...r.items] : r.items
      suite = r.next_cursor
    } catch (e) { echec(e, 'L’historique est indisponible') } finally { loading = false }
  }

  $effect(() => {
    const o = onglet
    const p = periode
    let gone = false
    loading = true
    const fin = () => { if (!gone) loading = false }
    if (o === 'recent') void plus(null)
    else if (o === 'tracks') sillonApi.topPistes(p).then((r) => { if (!gone) pistes = r.items }).catch((e) => echec(e, 'Le classement est indisponible')).finally(fin)
    else sillonApi.topArtistes(p).then((r) => { if (!gone) artistes = r.items }).catch((e) => echec(e, 'Le classement est indisponible')).finally(fin)
    return () => { gone = true }
  })
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Historique</h2>
    <div class="v-seg" role="tablist" aria-label="Historique">
      {#each ONGLETS as o (o.id)}<button role="tab" aria-selected={onglet === o.id} onclick={() => (onglet = o.id)}>{o.label}</button>{/each}
    </div>
    {#if onglet !== 'recent'}
      <div class="v-seg" role="radiogroup" aria-label="Période">
        {#each PERIODES as p (p.id)}<button role="radio" aria-checked={periode === p.id} onclick={() => (periode = p.id)}>{p.label}</button>{/each}
      </div>
    {/if}
  </div>
  <section class="film-section">
    {#if loading && !(onglet === 'recent' ? ecoutes.length : onglet === 'tracks' ? pistes.length : artistes.length)}
      <Loader />
    {:else if onglet === 'recent'}
      {#if ecoutes.length}
        <SillonTracks pistes={ecoutes.map((e) => e.track)} cles={ecoutes.map((e) => e.id)} />
        {#if suite}<button class="plbls-button plbls-button--contour film-more" disabled={loading} onclick={() => void plus(suite)}>Afficher plus</button>{/if}
      {:else}
        <div class="v-empty"><Icon name="horloge" size={28} /><p class="v-note">Aucune écoute pour l’instant.</p></div>
      {/if}
    {:else if onglet === 'tracks'}
      {#if pistes.length}
        <ol class="sillon-top">
          {#each pistes as t, i (t.track.id)}
            <li>
              <button class="sillon-top-l" disabled={!t.track.available} onclick={() => lecteur.jouer(pistes.map((x) => x.track), i)} aria-label={`Lire : ${t.track.title}, ${t.track.artist.name}`}>
                <span class="sillon-top-n v-mono">{i + 1}</span>
                <span class="sillon-top-art"><SillonCover id={t.track.album.id} title={t.track.album.title} image={t.track.album.cover} size="small" /></span>
                <span class="grow sillon-top-t"><strong class="truncate">{t.track.title}</strong><span class="v-note truncate">{t.track.artist.name}</span></span>
                <span class="v-mono v-mute">{fois(t.plays)}</span>
              </button>
            </li>
          {/each}
        </ol>
      {:else}
        <div class="v-empty"><Icon name="trophee" size={28} /><p class="v-note">Aucune écoute comptée sur cette période.</p></div>
      {/if}
    {:else if artistes.length}
      <ol class="sillon-top">
        {#each artistes as a, i (a.artist.id)}
          <li>
            <a class="sillon-top-l" href={hrefs.musique('artist', a.artist.id)}>
              <span class="sillon-top-n v-mono">{i + 1}</span>
              <span class="sillon-top-art sillon-rond"><SillonCover id={a.artist.id} title={a.artist.name} image={a.artist.image} kind="artiste" size="small" /></span>
              <strong class="grow truncate">{a.artist.name}</strong>
              <span class="v-mono v-mute">{fois(a.plays)}</span>
            </a>
          </li>
        {/each}
      </ol>
    {:else}
      <div class="v-empty"><Icon name="trophee" size={28} /><p class="v-note">Aucune écoute comptée sur cette période.</p></div>
    {/if}
  </section>
</div>

<style>
  .sillon-top { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  .sillon-top-l { display: flex; align-items: center; gap: 14px; width: 100%; min-height: 52px; padding: 6px 10px; border: 0; border-radius: var(--r); background: transparent; color: inherit; font: inherit; text-align: left; text-decoration: none; cursor: pointer; }
  .sillon-top-l:hover:not(:disabled) { background: var(--papier-2); }
  .sillon-top-l:focus-visible { outline: none; box-shadow: var(--anneau); }
  .sillon-top-n { flex: none; width: 24px; color: var(--gris); text-align: right; }
  .sillon-top-art { position: relative; flex: none; width: 40px; height: 40px; overflow: hidden; border-radius: var(--r-s); }
  .sillon-top-art.sillon-rond { border-radius: 50%; }
  .sillon-top-t { display: grid; min-width: 0; line-height: 1.3; }
</style>
