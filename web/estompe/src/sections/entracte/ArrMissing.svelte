<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { entracteApi } from '../../lib/api'
  import { confirmer } from '../../lib/confirm.svelte'
  import { notify } from '../../lib/ui'
  import { episodeCode, serviceName, since, splitMissing, titleLabel, type ArrError, type ArrMissing, type ArrService, type ReleaseTarget } from './arr'

  let { onsheet, onreleases }: { onsheet: (service: ArrService, id: number) => void; onreleases: (t: ReleaseTarget) => void } = $props()

  let items = $state<ArrMissing[] | null>(null)
  let totals = $state<Partial<Record<ArrService, number>>>({})
  let errors = $state<ArrError[]>([])
  let type = $state<'' | ArrService>('')
  let busy = $state('')

  const parts = $derived(splitMissing(items ?? []))
  const total = (s: ArrService) => totals[s] ?? 0
  const recherche = (m: ArrMissing) => `dernière recherche : ${since(m.last_search)}`

  async function load() {
    try {
      const r = await entracteApi.arrMissing()
      items = r.items
      totals = r.totals
      errors = r.errors
    } catch (e) { items = []; notify.error(e, 'Les titres manquants sont indisponibles') }
  }
  $effect(() => { void load() })

  async function search(cle: string, service: ArrService, corps: Parameters<typeof entracteApi.arrSearch>[1], quoi: string) {
    busy = cle
    try { await entracteApi.arrSearch(service, corps); notify.ok(`Recherche lancée : ${quoi}.`) } catch (e) { notify.error(e, 'La recherche n’a pas pu être lancée') } finally { busy = '' }
  }

  async function searchAll() {
    if (!(await confirmer({ titre: 'Tout rechercher', message: 'Radarr et Sonarr cherchent chaque titre surveillé sans fichier et récupèrent ce que leurs profils acceptent. Cela sollicite tous les indexeurs.', action: 'Lancer la recherche' }))) return
    busy = 'tout'
    try {
      for (const s of ['radarr', 'sonarr'] as const) if ((!type || type === s) && total(s) > 0) await entracteApi.arrSearch(s, { missing: true })
      notify.ok('Recherche de tous les manquants lancée.')
    } catch (e) { notify.error(e, 'La recherche n’a pas pu être lancée') } finally { busy = '' }
  }
</script>

<section class="film-section">
  <div class="film-filters">
    <div class="grow adm-body"><h2 class="v-titre film-filters-title">Manquants</h2><span class="v-note">Titres surveillés, déjà sortis, sans fichier.</span></div>
    <div class="v-seg adm-filtres" role="group" aria-label="Type">
      <button aria-pressed={type === ''} onclick={() => (type = '')}>Tout · {total('radarr') + total('sonarr')}</button>
      <button aria-pressed={type === 'radarr'} onclick={() => (type = 'radarr')}>Films · {total('radarr')}</button>
      <button aria-pressed={type === 'sonarr'} onclick={() => (type = 'sonarr')}>Épisodes · {total('sonarr')}</button>
    </div>
    <button class="plbls-button plbls-button--compact" disabled={!!busy || !items?.length} onclick={() => void searchAll()}><Icon name="cherche" size={14} /> Tout rechercher</button>
  </div>
  {#each errors as e (e.service)}<p class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> {serviceName(e.service)} ne répond pas : {e.message}</p>{/each}

  {#if !items}
    <Loader />
  {:else if !items.length}
    <div class="v-empty"><Icon name="ok" size={28} /><p class="v-note">Rien ne manque.</p></div>
  {:else}
    {#if type !== 'sonarr' && parts.movies.length}
      <h3 class="v-eyebrow">Films · {total('radarr')}</h3>
      <ul class="adm-list">
        {#each parts.movies as m (m.title_id)}
          <li class="adm-item">
            <span class="grow adm-body"><strong>{titleLabel(m)}</strong><span class="v-note">{recherche(m)}</span></span>
            <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={busy === `r${m.title_id}`} onclick={() => void search(`r${m.title_id}`, 'radarr', { title: m.title_id! }, m.title ?? 'film')}>Rechercher</button>
            <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => onreleases({ service: 'radarr', title: m.title_id!, label: titleLabel(m) })}>Choisir une release…</button>
            <button class="plbls-button plbls-button--compact plbls-button--contour" aria-label={`Gérer ${m.title ?? 'ce film'}`} onclick={() => onsheet('radarr', m.title_id!)}>Gérer</button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if type !== 'radarr' && parts.groups.length}
      <h3 class="v-eyebrow">Séries · {total('sonarr')} épisode{total('sonarr') > 1 ? 's' : ''}</h3>
      <ul class="adm-list">
        {#each parts.groups as g (g.key)}
          <li class="adm-item adm-pile">
            <div class="adm-ligne">
              <span class="grow adm-body"><strong>{g.title} · Saison {g.season}</strong><span class="v-note">{g.episodes.length} épisode{g.episodes.length > 1 ? 's' : ''} manquant{g.episodes.length > 1 ? 's' : ''}</span></span>
              <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={busy === g.key} onclick={() => void search(g.key, 'sonarr', { title: g.title_id, season: g.season }, `${g.title}, saison ${g.season}`)}>Rechercher la saison</button>
              <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => onreleases({ service: 'sonarr', title: g.title_id, season: g.season, label: `${g.title} · Saison ${g.season}` })}>Choisir un pack…</button>
              <button class="plbls-button plbls-button--compact plbls-button--contour" aria-label={`Gérer ${g.title}`} onclick={() => onsheet('sonarr', g.title_id)}>Gérer</button>
            </div>
            <ul class="adm-sous">
              {#each g.episodes as m (m.episode?.id)}
                {@const code = episodeCode(m.episode)}
                <li>
                  <span class="adm-code">{code}</span>
                  <span class="grow">{m.episode?.title ?? ''}</span>
                  <span class="v-note">{recherche(m)}</span>
                  {#if m.episode?.id}
                    <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={busy === `e${m.episode.id}`} aria-label={`Rechercher ${code}`} onclick={() => void search(`e${m.episode!.id}`, 'sonarr', { episodes: [m.episode!.id!] }, `${g.title} ${code}`)}>Rechercher</button>
                    <button class="plbls-button plbls-button--compact plbls-button--contour" aria-label={`Choisir une release pour ${code}`} onclick={() => onreleases({ service: 'sonarr', episode: m.episode!.id!, label: `${g.title} · ${code}` })}>Choisir…</button>
                  {/if}
                </li>
              {/each}
            </ul>
          </li>
        {/each}
      </ul>
    {/if}
    {#if items.length < total('radarr') + total('sonarr')}<p class="v-note">{items.length} affichés sur {total('radarr') + total('sonarr')} : les plus récents d’abord.</p>{/if}
  {/if}
</section>
