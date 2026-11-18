<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { entracteApi, filmsApi } from '../../lib/api'
  import { confirmer, saisir } from '../../lib/confirm.svelte'
  import { notify } from '../../lib/ui'
  import AcceptModal from './AcceptModal.svelte'
  import { reasonLabel, type FilmReject } from './films'
  import { introAnalysed, introLabel, introPending, introTone, seasonsLabel, stateLabel, stateTone, type EntracteRequest, type EntracteStatus, type IntroAdmin, type IntroSeries, type RequestState } from './entracte'

  let tab = $state<'requests' | 'rejected' | 'intros' | 'status'>('requests')
  let filtre = $state<'' | RequestState>('en_attente')
  let requester = $state('')
  let requests = $state<EntracteRequest[] | null>(null)
  let rejected = $state<FilmReject[] | null>(null)
  let status = $state<EntracteStatus | null>(null)
  let busy = $state('')
  let intros = $state<IntroAdmin | null>(null)
  let accepting = $state<EntracteRequest | null>(null)

  const date = (s: string | null) => (s ? new Date(s).toLocaleString('fr-FR', { dateStyle: 'medium', timeStyle: 'short' }) : 'jamais')

  async function loadRequests() {
    try { requests = (await entracteApi.adminRequests({ state: filtre, requester: requester.trim() })).items } catch (e) { requests = []; notify.error(e, 'Les demandes sont indisponibles') }
  }
  $effect(() => { if (tab === 'requests') { filtre; requester; void loadRequests() } })
  $effect(() => { if (tab === 'rejected') void filmsApi.rejects().then((r) => (rejected = r.items)).catch((e) => { rejected = []; notify.error(e, 'Liste indisponible') }) })
  $effect(() => { if (tab === 'status') void entracteApi.status().then((s) => (status = s)).catch((e) => notify.error(e, 'État indisponible')) })

  async function loadIntros() {
    try { intros = await entracteApi.intros() } catch (e) { notify.error(e, 'Les intros sont indisponibles') }
  }
  $effect(() => { if (tab === 'intros') void loadIntros() })

  async function setIntroMode(auto: boolean) {
    busy = 'mode'
    try {
      await entracteApi.introMode(auto ? 'automatique' : 'manuel')
      notify.ok(auto ? 'Détection automatique activée.' : 'Détection manuelle : rien ne démarre seul.')
    } catch (e) { notify.error(e, 'Le mode n’a pas pu être changé') } finally { busy = ''; await loadIntros() }
  }

  async function analyseIntros(s: IntroSeries) {
    busy = s.id
    try {
      const r = await entracteApi.introAnalyze(s.id)
      notify.ok(r.queued ? `« ${s.title} » : ${r.queued} saison${r.queued > 1 ? 's' : ''} en file.` : `« ${s.title} » est déjà en file.`)
    } catch (e) { notify.error(e, 'L’analyse n’a pas pu être lancée') } finally { busy = ''; await loadIntros() }
  }

  async function clearIntros(s: IntroSeries) {
    if (!(await confirmer({ titre: `Effacer les intros de « ${s.title} »`, message: 'Le bouton « Passer l’intro » disparaît pour cette série jusqu’à la prochaine analyse.', action: 'Effacer', destructif: true }))) return
    busy = s.id
    try { await entracteApi.introClear(s.id); notify.ok('Intros effacées.') } catch (e) { notify.error(e, 'Les intros n’ont pas pu être effacées') } finally { busy = ''; await loadIntros() }
  }

  function accepted(out: EntracteRequest) {
    if (out.send_error) notify.fail(`Acceptée, mais l’envoi a échoué : ${out.send_error}`)
    else notify.ok(`« ${out.title} » envoyé à ${out.kind === 'movie' ? 'Radarr' : 'Sonarr'}.`)
    void loadRequests()
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
      <button aria-pressed={tab === 'intros'} onclick={() => (tab = 'intros')}>Intros</button>
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
              <button class="plbls-button plbls-button--compact" disabled={busy === r.id} onclick={() => (accepting = r)}>{r.state === 'acceptee' ? 'Renvoyer' : 'Accepter'}</button>
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
  {:else if tab === 'intros'}
    {#if !intros}
      <Loader />
    {:else}
      <section class="film-section">
        <div class="film-section-head"><h2 class="v-titre">Passer l’intro</h2><button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void loadIntros()}><Icon name="actualise" size={14} /> Actualiser</button></div>
        <label class="adm-switch">
          <input type="checkbox" role="switch" checked={intros.mode === 'automatique'} disabled={busy === 'mode'} onchange={(e) => void setIntroMode(e.currentTarget.checked)} />
          <span><span class="v-lbl">Détection automatique</span><span class="v-note">Après chaque synchronisation, les saisons jamais analysées sont mises en file. Désactivée : l’analyse se lance série par série, ci-dessous.</span></span>
        </label>
        {#if !intros.available}<p class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> Chromaprint est indisponible sur le serveur (ni ffmpeg, ni fpcalc) : aucune analyse ne peut aboutir.</p>{/if}
      </section>
      {#if !intros.items.length}
        <div class="v-empty"><Icon name="film" size={28} /><p class="v-note">Aucune série dans la bibliothèque.</p></div>
      {:else}
        <ul class="adm-list">
          {#each intros.items as s (s.id)}
            <li class="adm-item">
              <span class="grow adm-body">
                <strong>{s.title}{s.year ? ` (${s.year})` : ''}</strong>
                {#each s.seasons as x (x.number)}
                  <span class="adm-season"><span class="v-note">Saison {x.number} · {x.episodes} épisode{x.episodes > 1 ? 's' : ''}{x.updated_at && x.state !== 'jamais' ? ` · ${date(x.updated_at)}` : ''}</span><span class="v-tag {introTone(x.state)}">{introLabel(x)}</span></span>
                  {#if x.error}<span class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> {x.error}</span>{/if}
                {/each}
              </span>
              <button class="plbls-button plbls-button--compact" disabled={busy === s.id || introPending(s)} onclick={() => void analyseIntros(s)}>{introAnalysed(s) ? 'Réanalyser' : 'Analyser'}</button>
              {#if introAnalysed(s)}
                <button class="plbls-button plbls-button--compact plbls-button--contour plbls-button--danger" disabled={busy === s.id} aria-label={`Effacer les intros de ${s.title}`} onclick={() => void clearIntros(s)}>Effacer</button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
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
{#if accepting}<AcceptModal request={accepting} onclose={() => (accepting = null)} ondone={accepted} />{/if}

<style>
  .adm-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .adm-item { display: flex; align-items: center; flex-wrap: wrap; gap: var(--espace-2); padding: var(--espace-2) var(--espace-3); border-radius: var(--r); background: var(--surface); box-shadow: var(--o1); }
  .adm-body { display: flex; flex-direction: column; min-width: 0; }
  .adm-season { display: flex; flex-wrap: wrap; align-items: center; gap: var(--espace-2); }
  .adm-switch { display: flex; align-items: center; gap: 10px; max-width: 560px; cursor: pointer; }
  .adm-switch input { flex: none; width: 18px; height: 18px; accent-color: var(--rose-fonce); }
  .adm-switch > span { display: grid; gap: 2px; flex: 1; min-width: 0; }
  .adm-err { display: inline-flex; align-items: center; gap: 4px; color: var(--danger, inherit); }
</style>
