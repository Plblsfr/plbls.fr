<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import SillonCover from './SillonCover.svelte'
  import { echec } from './sillon.svelte'
  import { anneeEtType, dureeTotale, octets, refusAjout, sillonApi, vignetteAbsent, type ResultatsAjout } from './sillon'

  let q = $state('')
  let res = $state<ResultatsAjout | null>(null)
  let loading = $state(false)
  let envoi = $state('')
  let refus = $state<Record<string, string>>({})
  let envoyes = $state<string[]>([])
  let tour = 0

  async function chercher(e: SubmitEvent) {
    e.preventDefault()
    const t = q.trim()
    if (!t) return
    const mine = ++tour
    loading = true
    refus = {}
    try {
      const r = await sillonApi.chercherAjout(t)
      if (mine === tour) res = r
    } catch (err) { if (mine === tour) echec(err, 'La recherche est indisponible') } finally { if (mine === tour) loading = false }
  }

  async function ajouter(kind: 'album' | 'artist', mbid: string, titre: string) {
    envoi = mbid
    const { [mbid]: _, ...reste } = refus
    refus = reste
    try {
      const a = await sillonApi.ajouter(kind, mbid)
      envoyes = [...envoyes, mbid]
      notify.ok(a.existing ? `« ${titre} » est déjà en cours d’ajout.` : `« ${titre} » est envoyé au téléchargement.`)
    } catch (e) { refus = { ...refus, [mbid]: refusAjout(e) } } finally { envoi = '' }
  }

  const etat = (o: { in_library: boolean; in_progress: boolean; excluded: boolean; mbid: string }) =>
    o.in_library ? 'Dans la bibliothèque' : o.excluded ? 'Retiré par un administrateur' : o.in_progress || envoyes.includes(o.mbid) ? 'En cours d’ajout' : ''
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Ajouter de la musique</h2>
    <form class="sillon-ajout-form" onsubmit={chercher}>
      <span class="film-search"><Icon name="cherche" size={16} /><input class="plbls-input" type="search" aria-label="Chercher un artiste ou un album à ajouter" placeholder="Artiste ou album absent…" maxlength="100" bind:value={q} /></span>
      <button class="plbls-button" disabled={!q.trim() || loading}>Chercher</button>
    </form>
  </div>
  {#if loading && !res}
    <Loader />
  {:else if res && !res.artists.length && !res.albums.length}
    <div class="v-empty"><Icon name="cherche" size={28} /><p class="v-note">Rien trouvé pour « {q.trim()} ».</p></div>
  {:else if res}
    {#if res.albums.length}
      <section class="film-section">
        <h2 class="v-titre">Albums</h2>
        <ul class="sillon-absents">
          {#each res.albums as a (a.mbid)}
            {@const e = etat(a)}
            <li class="sillon-absent-l">
              <span class="sillon-absent-art"><SillonCover {...vignetteAbsent(a)} title={a.title ?? ''} size="small" /></span>
              <span class="grow sillon-absent-t">
                <strong class="truncate">{a.title ?? 'Sans titre'}</strong>
                <span class="v-note truncate">{[a.artist.name, anneeEtType({ year: a.year, type: a.type, secondary_types: a.secondary_types }), a.duration ? dureeTotale(a.duration) : '', a.estimated_size ? `≈ ${octets(a.estimated_size)}` : ''].filter(Boolean).join(' · ')}</span>
              </span>
              {#if e}<span class="v-tag" class:ok={a.in_library}>{e}</span>{:else}<button class="v-btn sm" disabled={envoi === a.mbid} onclick={() => void ajouter('album', a.mbid, a.title ?? 'Album')} aria-label={`Ajouter l’album ${a.title ?? ''}`}><Icon name="plus" size={14} />Ajouter</button>{/if}
              {#if refus[a.mbid]}<p class="sillon-refus" role="alert">{refus[a.mbid]}</p>{/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if res.artists.length}
      <section class="film-section">
        <h2 class="v-titre">Artistes</h2>
        <ul class="sillon-absents">
          {#each res.artists as a (a.mbid)}
            {@const e = etat(a)}
            <li class="sillon-absent-l">
              <span class="sillon-absent-art sillon-rond"><SillonCover {...vignetteAbsent(a)} title={a.name ?? ''} kind="artiste" size="small" /></span>
              <span class="grow sillon-absent-t">
                <strong class="truncate">{a.name ?? 'Sans nom'}</strong>
                <span class="v-note truncate">{[a.disambiguation, a.type].filter(Boolean).join(' · ') || 'Albums studio'}</span>
              </span>
              {#if e}<span class="v-tag" class:ok={a.in_library}>{e}</span>{:else}<button class="v-btn sm" disabled={envoi === a.mbid} onclick={() => void ajouter('artist', a.mbid, a.name ?? 'Artiste')} aria-label={`Ajouter l’artiste ${a.name ?? ''}`}><Icon name="plus" size={14} />Ajouter</button>{/if}
              {#if refus[a.mbid]}<p class="sillon-refus" role="alert">{refus[a.mbid]}</p>{/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {:else}
    <p class="v-note">Cherche un album ou un artiste qui n’est pas encore dans la bibliothèque : il est téléchargé tout de suite (albums studio pour un artiste). Suis-les dans <a href={hrefs.musique('additions')}>Mes ajouts</a>.</p>
  {/if}
</div>

<style>
  .sillon-ajout-form { display: flex; flex: 1 1 420px; grid-column: 1 / -1; gap: 8px; min-width: 0; }
  .sillon-ajout-form .film-search { flex: 1 1 auto; min-width: 0; }
  .sillon-absents { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  .sillon-absent-l { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 14px; min-height: 56px; padding: 6px 10px; border-radius: var(--r); }
  .sillon-absent-l:hover { background: var(--papier-2); }
  .sillon-absent-art { position: relative; flex: none; width: 44px; height: 44px; overflow: hidden; border-radius: var(--r-s); }
  .sillon-absent-art.sillon-rond { border-radius: 50%; }
  .sillon-absent-t { display: grid; flex: 1 1 0; min-width: 0; line-height: 1.3; }
  .sillon-refus { flex-basis: 100%; margin: 0 0 4px 58px; padding: 8px 12px; border-radius: var(--r-s); background: var(--danger-fond); color: var(--danger); font-size: 13px; }
  @media (max-width: 720px) { .sillon-refus { margin-left: 0; } }
</style>
