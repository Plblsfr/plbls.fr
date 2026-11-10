<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { entracteApi, filmsApi } from '../../lib/api'
  import { saisir } from '../../lib/confirm.svelte'
  import { notify } from '../../lib/ui'
  import { reasonLabel, type FilmReject } from '../../blocks/films/films'
  import { seasonsLabel, stateLabel, stateTone, type EntracteRequest, type EntracteStatus, type RequestState } from './entracte'

  let tab = $state<'requests' | 'rejected' | 'status'>('requests')
  let filtre = $state<'' | RequestState>('en_attente')
  let requester = $state('')
  let requests = $state<EntracteRequest[] | null>(null)
  let rejected = $state<FilmReject[] | null>(null)
  let status = $state<EntracteStatus | null>(null)
  let busy = $state('')

  const date = (s: string | null) => (s ? new Date(s).toLocaleString('fr-FR', { dateStyle: 'medium', timeStyle: 'short' }) : 'jamais')

  async function loadRequests() {
    try { requests = (await entracteApi.adminRequests({ state: filtre, requester: requester.trim() })).items } catch (e) { requests = []; notify.error(e, 'Les demandes sont indisponibles') }
  }
  $effect(() => { if (tab === 'requests') { filtre; requester; void loadRequests() } })
  $effect(() => { if (tab === 'rejected') void filmsApi.rejects().then((r) => (rejected = r.items)).catch((e) => { rejected = []; notify.error(e, 'Liste indisponible') }) })
  $effect(() => { if (tab === 'status') void entracteApi.status().then((s) => (status = s)).catch((e) => notify.error(e, 'État indisponible')) })

  async function accept(r: EntracteRequest) {
    busy = r.id
    try {
      const out = await entracteApi.accept(r.id)
      if (out.send_error) notify.fail(`Acceptée, mais l’envoi a échoué : ${out.send_error}`)
      else notify.ok(`« ${r.title} » envoyé à ${r.kind === 'movie' ? 'Radarr' : 'Sonarr'}.`)
      await loadRequests()
    } catch (e) { notify.error(e, 'La demande n’a pas pu être acceptée') } finally { busy = '' }
  }

  async function refuse(r: EntracteRequest) {
    const reason = await saisir({ titre: `Refuser « ${r.title} »`, message: 'Le motif est envoyé aux personnes qui l’ont demandé.', action: 'Refuser', destructif: true, saisie: { label: 'Motif' } })
    if (reason === null) return
    if (!reason.trim()) { notify.fail('Le motif est obligatoire.'); return }
    busy = r.id
    try { await entracteApi.refuse(r.id, reason.trim()); notify.ok('Demande refusée.'); await loadRequests() } catch (e) { notify.error(e, 'La demande n’a pas pu être refusée') } finally { busy = '' }
  }

  async function sync() {
    try { await entracteApi.sync(); notify.ok('Synchronisation lancée.') } catch (e) { notify.error(e, 'Synchronisation impossible') }
  }
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Administration d’Entracte</h2>
    <div class="v-seg" role="group" aria-label="Rubrique">
      <button aria-pressed={tab === 'requests'} onclick={() => (tab = 'requests')}>Demandes</button>
      <button aria-pressed={tab === 'rejected'} onclick={() => (tab = 'rejected')}>Fichiers écartés</button>
      <button aria-pressed={tab === 'status'} onclick={() => (tab = 'status')}>État</button>
    </div>
  </div>

  {#if tab === 'requests'}
    <div class="film-filters">
      <label class="film-filter"><span class="v-sr">État</span>
        <select class="plbls-select film-select" bind:value={filtre}>
          <option value="">Tous les états</option>
          <option value="en_attente">En attente</option><option value="acceptee">Acceptées</option><option value="refusee">Refusées</option><option value="disponible">Disponibles</option>
        </select>
      </label>
      <span class="film-search"><Icon name="perso" size={16} /><input class="plbls-input" type="search" aria-label="Demandeur" placeholder="Demandeur" maxlength="60" bind:value={requester} /></span>
    </div>
    {#if !requests}
      <Loader />
    {:else if !requests.length}
      <div class="v-empty"><Icon name="ok" size={28} /><p class="v-note">Aucune demande.</p></div>
    {:else}
      <ul class="adm-list">
        {#each requests as r (r.id)}
          <li class="adm-item">
            <span class="grow adm-body">
              <strong>{r.title}{r.year ? ` (${r.year})` : ''}</strong>
              <span class="v-note">{seasonsLabel(r)} · {date(r.created_at)} · {(r.requesters ?? []).map((x) => x.name || 'quelqu’un').join(', ')}</span>
              {#if r.reason}<span class="v-note">Motif : {r.reason}</span>{/if}
              {#if r.send_error}<span class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> Envoi à {r.kind === 'movie' ? 'Radarr' : 'Sonarr'} en échec : {r.send_error}</span>{/if}
            </span>
            <span class="v-tag {stateTone(r.state)}">{stateLabel(r.state)}</span>
            {#if r.state === 'en_attente' || (r.state === 'acceptee' && !r.sent)}
              <button class="plbls-button plbls-button--compact" disabled={busy === r.id} onclick={() => void accept(r)}>{r.state === 'acceptee' ? 'Renvoyer' : 'Accepter'}</button>
            {/if}
            {#if r.state === 'en_attente' || r.state === 'acceptee'}
              <button class="plbls-button plbls-button--compact plbls-button--contour plbls-button--danger" disabled={busy === r.id} onclick={() => void refuse(r)}>Refuser</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {:else if tab === 'rejected'}
    {#if !rejected}
      <Loader />
    {:else if !rejected.length}
      <div class="v-empty"><Icon name="ok" size={28} /><p class="v-note">Aucun fichier écarté.</p></div>
    {:else}
      <ul class="adm-list">
        {#each rejected as f (f.name + f.reason)}
          <li class="adm-item"><span class="grow adm-body"><strong class="truncate">{f.name}</strong><span class="v-note">{reasonLabel(f.reason)} · {date(f.seen_at)}</span></span></li>
        {/each}
      </ul>
    {/if}
  {:else}
    {#if !status}
      <Loader />
    {:else}
      <section class="film-section">
        <div class="film-section-head"><h2 class="v-titre">Synchronisations</h2><button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void sync()}><Icon name="actualise" size={14} /> Synchroniser</button></div>
        <ul class="adm-list">
          {#each status.sync as s (s.source)}
            <li class="adm-item">
              <span class="grow adm-body"><strong>{s.source === 'tmdb' ? 'TMDB' : s.source === 'radarr' ? 'Radarr' : s.source === 'sonarr' ? 'Sonarr' : s.source}</strong><span class="v-note">Dernière réussite : {date(s.succeeded_at)}</span>{#if s.error}<span class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> {s.error}</span>{/if}</span>
              <span class="v-tag {s.error ? 'dgr' : 'ok'}">{s.error ? 'En erreur' : 'OK'}</span>
            </li>
          {/each}
        </ul>
      </section>
      <section class="film-section">
        <h2 class="v-titre">Transcodage</h2>
        <p class="v-note">Mode demandé : {status.transcoding.requested} · mode actif : {status.transcoding.active} · transcodages en cours : {status.transcoding.running}</p>
        {#if status.transcoding.message}<p class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> {status.transcoding.message}</p>{/if}
      </section>
    {/if}
  {/if}
</div>

<style>
  .adm-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .adm-item { display: flex; align-items: center; flex-wrap: wrap; gap: var(--espace-2); padding: var(--espace-2) var(--espace-3); border-radius: var(--r); background: var(--surface); box-shadow: var(--o1); }
  .adm-body { display: flex; flex-direction: column; min-width: 0; }
  .adm-err { display: inline-flex; align-items: center; gap: 4px; color: var(--danger, inherit); }
</style>
