<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { entracteApi } from '../../lib/api'
  import { confirmer } from '../../lib/confirm.svelte'
  import { notify } from '../../lib/ui'
  import ArrImport from './ArrImport.svelte'
  import { bytes, eventLabel, eventTone, queueLabel, queueTone, serviceName, since, titleLabel, type ArrBlocked, type ArrError, type ArrHistory, type ArrQueueItem, type ArrService } from './arr'

  let { queue, errors, onreload, onsheet }: { queue: ArrQueueItem[] | null; errors: ArrError[]; onreload: () => void; onsheet: (service: ArrService, id: number) => void } = $props()

  let vue = $state<'queue' | 'history' | 'blocklist'>('queue')
  let service = $state<'' | ArrService>('')
  let history = $state<ArrHistory[] | null>(null)
  let blocked = $state<ArrBlocked[] | null>(null)
  let busy = $state('')
  let importing = $state<ArrQueueItem | null>(null)

  const garde = <T extends { service: ArrService }>(l: T[] | null) => (l ?? []).filter((x) => !service || x.service === service)
  const cle = (x: { service: ArrService; id: number }) => `${x.service}:${x.id}`

  async function loadHistory() {
    try { history = (await entracteApi.arrHistory()).items } catch (e) { history = []; notify.error(e, 'L’historique est indisponible') }
  }
  async function loadBlocked() {
    try { blocked = (await entracteApi.arrBlocklist()).items } catch (e) { blocked = []; notify.error(e, 'La liste noire est indisponible') }
  }
  $effect(() => { if (vue === 'history') void loadHistory(); else if (vue === 'blocklist') void loadBlocked() })

  async function remove(q: ArrQueueItem, autre: boolean) {
    const nom = titleLabel(q)
    if (!(await confirmer(autre
      ? { titre: `Autre version de « ${nom} »`, message: `Le téléchargement est retiré, cette release passe sur la liste noire et ${serviceName(q.service)} relance une recherche.`, action: 'Chercher une autre version' }
      : { titre: `Retirer « ${nom} »`, message: 'Le téléchargement est retiré de la file et du client. Rien n’est bloqué ni relancé.', action: 'Retirer', destructif: true }))) return
    busy = cle(q)
    try {
      await entracteApi.arrRemove(q.service, q.id, { blocklist: autre, search: autre })
      notify.ok(autre ? 'Release écartée, nouvelle recherche lancée.' : 'Téléchargement retiré.')
    } catch (e) { notify.error(e, 'Le téléchargement n’a pas pu être retiré') } finally { busy = ''; onreload() }
  }

  async function unblock(b: ArrBlocked) {
    busy = cle(b)
    try { await entracteApi.arrUnblock(b.service, b.id); notify.ok('Release sortie de la liste noire.') } catch (e) { notify.error(e, 'La release n’a pas pu être retirée de la liste') } finally { busy = ''; await loadBlocked() }
  }

  async function search(h: ArrHistory) {
    if (h.title_id === null) return
    try {
      await entracteApi.arrSearch(h.service, h.episode?.id ? { episodes: [h.episode.id] } : { title: h.title_id })
      notify.ok('Recherche lancée.')
    } catch (e) { notify.error(e, 'La recherche n’a pas pu être lancée') }
  }
</script>

<section class="film-section">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Téléchargements</h2>
    <div class="v-seg adm-filtres" role="group" aria-label="Vue">
      <button aria-pressed={vue === 'queue'} onclick={() => (vue = 'queue')}>File{queue?.length ? ` · ${queue.length}` : ''}</button>
      <button aria-pressed={vue === 'history'} onclick={() => (vue = 'history')}>Historique</button>
      <button aria-pressed={vue === 'blocklist'} onclick={() => (vue = 'blocklist')}>Liste noire</button>
    </div>
    <div class="v-seg adm-filtres" role="group" aria-label="Service">
      <button aria-pressed={service === ''} onclick={() => (service = '')}>Tout</button>
      <button aria-pressed={service === 'radarr'} onclick={() => (service = 'radarr')}>Films</button>
      <button aria-pressed={service === 'sonarr'} onclick={() => (service = 'sonarr')}>Séries</button>
    </div>
  </div>
  {#each errors as e (e.service)}<p class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> {serviceName(e.service)} ne répond pas : {e.message}</p>{/each}

  {#if vue === 'queue'}
    {#if !queue}
      <Loader />
    {:else if !garde(queue).length}
      <div class="v-empty"><Icon name="ok" size={28} /><p class="v-note">Aucun téléchargement en cours.</p></div>
    {:else}
      <ul class="adm-list">
        {#each garde(queue) as q (cle(q))}
          <li class="adm-item" class:adm-bloque={q.state === 'blocked'}>
            <span class="grow adm-body dl-corps">
              <strong>{titleLabel(q)}{q.episode?.title ? ` · ${q.episode.title}` : ''}</strong>
              <span class="adm-mono v-mute">{q.release}</span>
              <span class="adm-tags">
                {#if q.quality}<span class="v-tag encre">{q.quality}</span>{/if}
                <span class="v-tag">{serviceName(q.service)}</span>
                <span class="v-note">{[q.indexer, q.client, bytes(q.size)].filter(Boolean).join(' · ')}</span>
              </span>
              {#each q.messages as m (m)}<span class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> {m}</span>{/each}
            </span>
            <span class="dl-avance">
              <span class="adm-ligne dl-etat"><span class="v-tag {queueTone(q.state)}">{queueLabel(q.state)}</span><span class="adm-mono">{q.progress} %</span></span>
              <span class="adm-jauge" class:dgr={q.state === 'blocked'} class:att={q.state === 'queued' || q.state === 'paused'} role="progressbar" aria-label={`Avancement de ${titleLabel(q)}`} aria-valuemin="0" aria-valuemax="100" aria-valuenow={q.progress}><span style:width="{q.progress}%"></span></span>
              {#if q.eta && q.state === 'downloading'}<span class="v-note">fin {since(q.eta)}</span>{/if}
            </span>
            <span class="adm-actions">
              {#if q.state === 'blocked' && q.importable}<button class="plbls-button plbls-button--compact" disabled={busy === cle(q)} onclick={() => (importing = q)}>Importer à la main…</button>{/if}
              <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={busy === cle(q)} onclick={() => void remove(q, true)}>Autre version</button>
              <button class="plbls-button plbls-button--compact plbls-button--contour plbls-button--danger" disabled={busy === cle(q)} aria-label={`Retirer ${titleLabel(q)}`} onclick={() => void remove(q, false)}>Retirer</button>
              {#if q.title_id !== null}<button class="plbls-button plbls-button--compact plbls-button--contour" aria-label={`Gérer ${q.title ?? 'ce titre'}`} onclick={() => onsheet(q.service, q.title_id!)}>Gérer</button>{/if}
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  {:else if vue === 'history'}
    {#if !history}
      <Loader />
    {:else if !garde(history).length}
      <div class="v-empty"><Icon name="horloge" size={28} /><p class="v-note">Aucun évènement récent.</p></div>
    {:else}
      <ul class="adm-list">
        {#each garde(history) as h, i (i)}
          <li class="adm-item">
            <span class="v-tag {eventTone(h.event)}">{eventLabel(h.event)}</span>
            <span class="grow adm-body dl-corps"><strong>{titleLabel(h)}</strong>{#if h.release}<span class="adm-mono v-mute">{h.release}</span>{/if}{#if h.reason}<span class="v-note">{h.reason}</span>{/if}</span>
            <span class="v-note">{[h.quality, h.indexer, since(h.date)].filter(Boolean).join(' · ')}</span>
            {#if h.event === 'failed' && h.title_id !== null}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void search(h)}>Rechercher</button>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  {:else if !blocked}
    <Loader />
  {:else if !garde(blocked).length}
    <div class="v-empty"><Icon name="ok" size={28} /><p class="v-note">Aucune release sur la liste noire.</p></div>
  {:else}
    <p class="v-note">Une release sur cette liste n’est plus jamais récupérée seule. L’en sortir la rend de nouveau acceptable.</p>
    <ul class="adm-list">
      {#each garde(blocked) as b (cle(b))}
        <li class="adm-item">
          <span class="grow adm-body dl-corps"><strong>{titleLabel(b)}</strong>{#if b.release}<span class="adm-mono v-mute">{b.release}</span>{/if}{#if b.reason}<span class="v-note">{b.reason}</span>{/if}</span>
          <span class="v-note">{[b.quality, b.indexer, since(b.date)].filter(Boolean).join(' · ')}</span>
          <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={busy === cle(b)} onclick={() => void unblock(b)}>Sortir de la liste</button>
        </li>
      {/each}
    </ul>
  {/if}
</section>
{#if importing}<ArrImport item={importing} onclose={() => (importing = null)} ondone={onreload} />{/if}

<style>
  .dl-corps { flex-basis: 340px; gap: 4px; }
  .dl-avance { flex: 0 1 220px; display: flex; flex-direction: column; gap: 6px; }
  .dl-avance .adm-jauge { flex: none; }
  .dl-etat { justify-content: space-between; }
</style>
