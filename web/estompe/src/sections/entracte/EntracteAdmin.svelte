<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { entracteApi, filmsApi } from '../../lib/api'
  import { confirmer, saisir } from '../../lib/confirm.svelte'
  import { notify } from '../../lib/ui'
  import AcceptModal from './AcceptModal.svelte'
  import { reasonLabel, type FilmReject } from './films'
  import EntractePoster from './EntractePoster.svelte'
  import { films } from './films.svelte'
  import { failedSends, introAnalysed, introLabel, introPending, introTone, seasonsLabel, sourceLabel, stateLabel, stateTone, type EntracteRequest, type EntracteStatus, type IntroAdmin, type IntroSeries, type RequestState } from './entracte'

  let tab = $state<'requests' | 'rejected' | 'intros'>('requests')
  let filtre = $state<'' | RequestState>('en_attente')
  let requester = $state('')
  let requests = $state<EntracteRequest[] | null>(null)
  let failed = $state<EntracteRequest[]>([])
  let rejected = $state<FilmReject[] | null>(null)
  let status = $state<EntracteStatus | null>(null)
  let busy = $state('')
  let intros = $state<IntroAdmin | null>(null)
  let accepting = $state<EntracteRequest | null>(null)

  const date = (s: string | null) => (s ? new Date(s).toLocaleString('fr-FR', { dateStyle: 'medium', timeStyle: 'short' }) : 'jamais')

  async function loadRequests() {
    try { requests = (await entracteApi.adminRequests({ state: filtre, requester: requester.trim() })).items } catch (e) { requests = []; notify.error(e, 'Les demandes sont indisponibles') }
    void loadFailed()
  }
  async function loadFailed() {
    try {
      failed = failedSends((await entracteApi.adminRequests({ state: 'acceptee' })).items)
      films.pending = (await entracteApi.adminRequests({ state: 'en_attente' })).items.length
    } catch { failed = [] }
  }
  async function loadStatus() {
    try { status = await entracteApi.status() } catch (e) { notify.error(e, 'État indisponible') }
  }
  const FILTRES: ['' | RequestState, string][] = [['en_attente', 'En attente'], ['acceptee', 'Acceptées'], ['refusee', 'Refusées'], ['disponible', 'Disponibles'], ['', 'Toutes']]
  const visibles = $derived((requests ?? []).filter((r) => !failed.some((f) => f.id === r.id)))
  $effect(() => { if (tab === 'requests') { filtre; requester; void loadRequests() } })
  $effect(() => { if (tab === 'rejected') void filmsApi.rejects().then((r) => (rejected = r.items)).catch((e) => { rejected = []; notify.error(e, 'Liste indisponible') }) })
  $effect(() => { void loadStatus() })

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
    try { await entracteApi.sync(); notify.ok('Synchronisation lancée.'); await loadStatus() } catch (e) { notify.error(e, 'Synchronisation impossible') }
  }
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Administration d’Entracte</h2>
    <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void sync()}><Icon name="actualise" size={14} /> Synchroniser maintenant</button>
  </div>

  <section class="adm-etat" aria-label="État des services">
    {#if !status}
      <Loader />
    {:else}
      {#each status.sync as s (s.source)}
        <div class="adm-carte" class:erreur={!!s.error}>
          <span class="v-eyebrow">{sourceLabel(s.source)}</span>
          <strong>{#if s.error}<Icon name="alerte" size={16} /> En erreur{:else}<span class="adm-point" aria-hidden="true"></span> Joignable{/if}</strong>
          {#if s.error}<span class="adm-carte-note" role="alert">{s.error}</span><span class="adm-carte-note">Ce qui est déjà synchronisé reste servi.</span>
          {:else}<span class="v-note">Dernière réussite : {date(s.succeeded_at)}</span>{/if}
        </div>
      {/each}
      <div class="adm-carte" class:erreur={!!status.transcoding.message}>
        <span class="v-eyebrow">Transcodage</span>
        <strong>{status.transcoding.active}</strong>
        <span class="v-note">Demandé : {status.transcoding.requested} · en cours : {status.transcoding.running}</span>
        {#if status.transcoding.message}<span class="adm-carte-note" role="alert">{status.transcoding.message}</span>{/if}
      </div>
    {/if}
  </section>

  <div class="v-seg adm-onglets" role="group" aria-label="Rubrique">
    <button aria-pressed={tab === 'requests'} onclick={() => (tab = 'requests')}>Demandes</button>
    <button aria-pressed={tab === 'rejected'} onclick={() => (tab = 'rejected')}>Fichiers écartés</button>
    <button aria-pressed={tab === 'intros'} onclick={() => (tab = 'intros')}>Intros</button>
  </div>

  {#if tab === 'requests'}
    {#if failed.length}
      <section class="film-section">
        <h2 class="v-titre">Envoi en échec</h2>
        <ul class="adm-list adm-echecs">
          {#each failed as r (r.id)}
            <li class="adm-item">
            <span class="adm-art"><EntractePoster url={r.poster_url} title={r.title} seed={r.id} /></span>
            <span class="grow adm-body">
              <strong>{r.title}{r.year ? ` (${r.year})` : ''}</strong>
              <span class="v-note">{seasonsLabel(r)} · {date(r.created_at)} · {(r.requesters ?? []).map((x) => x.name || 'quelqu’un').join(', ')}</span>
              {#if r.reason}<span class="v-note">Motif : {r.reason}</span>{/if}
              {#if r.send_error}<span class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> Envoi à {r.kind === 'movie' ? 'Radarr' : 'Sonarr'} en échec : {r.send_error}</span>{/if}
            </span>
            {#if r.state === 'en_attente' || r.state === 'acceptee'}
              <button class="plbls-button plbls-button--compact plbls-button--contour plbls-button--danger" disabled={busy === r.id} onclick={() => void refuse(r)}>Refuser</button>
            {/if}
            {#if r.state === 'en_attente' || (r.state === 'acceptee' && !r.sent)}
              <button class="plbls-button plbls-button--compact" disabled={busy === r.id} onclick={() => (accepting = r)}>{r.state === 'acceptee' ? 'Renvoyer' : 'Accepter…'}</button>
            {/if}
          </li>
          {/each}
        </ul>
      </section>
    {/if}
    <section class="film-section">
      <div class="film-filters">
        <h2 class="v-titre film-filters-title">Demandes</h2>
        <div class="v-seg adm-filtres" role="group" aria-label="État">
          {#each FILTRES as [valeur, nom] (valeur)}<button aria-pressed={filtre === valeur} onclick={() => (filtre = valeur)}>{nom}{valeur === 'en_attente' && films.pending ? ` · ${films.pending}` : ''}</button>{/each}
        </div>
        <label class="film-search"><Icon name="perso" size={16} /><span class="v-sr">Demandeur</span><input class="plbls-input" type="search" placeholder="Demandeur" maxlength="60" bind:value={requester} /></label>
      </div>
      {#if !requests}
        <Loader />
      {:else if !visibles.length}
        <div class="v-empty"><Icon name="ok" size={28} /><p class="v-note">Aucune demande.</p></div>
      {:else}
        <ul class="adm-list">
          {#each visibles as r (r.id)}
            <li class="adm-item">
            <span class="adm-art"><EntractePoster url={r.poster_url} title={r.title} seed={r.id} /></span>
            <span class="grow adm-body">
              <strong>{r.title}{r.year ? ` (${r.year})` : ''}</strong>
              <span class="v-note">{seasonsLabel(r)} · {date(r.created_at)} · {(r.requesters ?? []).map((x) => x.name || 'quelqu’un').join(', ')}</span>
              {#if r.reason}<span class="v-note">Motif : {r.reason}</span>{/if}
              {#if r.send_error}<span class="v-note adm-err" role="alert"><Icon name="alerte" size={14} /> Envoi à {r.kind === 'movie' ? 'Radarr' : 'Sonarr'} en échec : {r.send_error}</span>{/if}
            </span>
            <span class="v-tag {stateTone(r.state)}">{stateLabel(r.state)}</span>
            {#if r.state === 'en_attente' || r.state === 'acceptee'}
              <button class="plbls-button plbls-button--compact plbls-button--contour plbls-button--danger" disabled={busy === r.id} onclick={() => void refuse(r)}>Refuser</button>
            {/if}
            {#if r.state === 'en_attente' || (r.state === 'acceptee' && !r.sent)}
              <button class="plbls-button plbls-button--compact" disabled={busy === r.id} onclick={() => (accepting = r)}>{r.state === 'acceptee' ? 'Renvoyer' : 'Accepter…'}</button>
            {/if}
          </li>
          {/each}
        </ul>
      {/if}
    </section>
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
            <li class="adm-item adm-intro">
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
  {/if}
</div>
{#if accepting}<AcceptModal request={accepting} onclose={() => (accepting = null)} ondone={accepted} />{/if}

<style>
  .adm-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .adm-item { display: flex; align-items: center; flex-wrap: wrap; gap: var(--espace-2) var(--espace-3); padding: var(--espace-3) var(--espace-4) var(--espace-3) var(--espace-3); border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); }
  .adm-echecs .adm-item { box-shadow: inset 0 0 0 1.5px var(--danger); }
  .adm-art { flex: none; width: 48px; aspect-ratio: 2 / 3; overflow: hidden; border-radius: var(--r-s); }
  .adm-art :global(.film-art-title), .adm-art :global(.film-disc) { display: none; }
  .adm-etat { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: var(--espace-3); }
  .adm-carte { display: flex; flex-direction: column; gap: var(--espace-1); min-width: 0; padding: var(--espace-3) var(--espace-4); border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); }
  .adm-carte strong { display: flex; align-items: center; gap: var(--espace-2); font-size: 17px; }
  .adm-carte.erreur { background: var(--danger-fond); box-shadow: inset 0 0 0 1.5px var(--danger); }
  .adm-carte.erreur strong { color: var(--danger); }
  .adm-carte-note { font-size: 13px; overflow-wrap: anywhere; }
  .adm-point { width: 10px; height: 10px; border-radius: 50%; background: var(--ok); }
  .adm-onglets { align-self: flex-start; }
  .adm-filtres { flex-wrap: wrap; border-radius: var(--r-l); }
  .adm-body { display: flex; flex-direction: column; min-width: 0; }
  .adm-intro .adm-body { flex-basis: 260px; gap: 2px; }
  .adm-season { display: flex; flex-wrap: wrap; align-items: center; gap: var(--espace-2); }
  .adm-switch { display: flex; align-items: center; gap: 10px; max-width: 560px; cursor: pointer; }
  .adm-switch input { flex: none; width: 18px; height: 18px; accent-color: var(--rose-fonce); }
  .adm-switch > span { display: grid; gap: 2px; flex: 1; min-width: 0; }
  .adm-err { display: inline-flex; align-items: center; gap: 4px; color: var(--danger, inherit); }
</style>
