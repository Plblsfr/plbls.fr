<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { confirmer } from '../../lib/confirm.svelte'
  import { notify } from '../../lib/ui'
  import { bytes, indexerState, problems, regieApi, serviceName, serviceState, sortIndexers, speed, type Blocked, type Indexer, type RegieStatus } from './regie'

  let status = $state<RegieStatus | null>(null)
  let failed = $state(false)
  let loading = $state(false)
  let busy = $state('')

  const date = (s: string | null) => (s ? new Date(s).toLocaleString('fr-FR', { dateStyle: 'medium', timeStyle: 'short' }) : '')
  const indexers = $derived(status ? sortIndexers(status.indexers) : [])
  const count = $derived(status ? problems(status) : 0)

  async function load() {
    loading = true
    try { status = await regieApi.status(); failed = false } catch (e) { failed = true; notify.error(e, 'L’état des services est indisponible') } finally { loading = false }
  }
  $effect(() => { void load() })

  async function test(i: Indexer) {
    busy = `i${i.id}`
    try {
      const r = await regieApi.testIndexer(i.id)
      if (r.ok) notify.ok(`« ${i.name} » répond.`)
      else notify.fail(`« ${i.name} » : ${r.message ?? 'test en échec'}`)
      await load()
    } catch (e) { notify.error(e, 'Le test n’a pas pu être lancé') } finally { busy = '' }
  }

  async function remove(b: Blocked, retry: boolean) {
    const ok = await confirmer({
      titre: retry ? `Chercher une autre version de « ${b.title} » ?` : `Retirer « ${b.title} » ?`,
      message: retry
        ? `Le téléchargement est retiré de ${serviceName(b.service)} et de qBittorrent, cette version est bloquée et une autre est cherchée.`
        : `Le téléchargement est retiré de ${serviceName(b.service)} et de qBittorrent, sans nouvelle recherche.`,
      action: retry ? 'Chercher une autre version' : 'Retirer',
      destructif: true,
    })
    if (!ok) return
    busy = `q${b.service}${b.id}`
    try {
      await regieApi.removeFromQueue(b.service, b.id, retry)
      notify.ok(retry ? 'Retiré, nouvelle recherche lancée.' : 'Retiré.')
      await load()
    } catch (e) { notify.error(e, 'Le téléchargement n’a pas pu être retiré') } finally { busy = '' }
  }
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Santé des médias</h2>
    {#if status}<span class="v-tag {count ? 'att' : 'ok'}">{count ? `${count} à traiter` : 'Tout va bien'}</span>{/if}
    <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={loading} onclick={() => void load()}><Icon name="actualise" size={14} /> Actualiser</button>
  </div>

  {#if !status}
    {#if failed}
      <div class="v-empty"><Icon name="alerte" size={28} /><p class="v-note">L’état des services n’a pas pu être lu.</p></div>
    {:else}
      <Loader />
    {/if}
  {:else}
    <section class="film-section">
      <h2 class="v-titre">Services</h2>
      <ul class="rg-list">
        {#each status.services as s (s.key)}
          {@const etat = serviceState(s)}
          <li class="rg-item">
            <span class="grow rg-body">
              <strong>{s.name}{#if s.version}<span class="v-mono v-mute rg-version">{s.version}</span>{/if}</strong>
              {#if s.transfer}<span class="v-note">Réception {speed(s.transfer.download_speed)} · envoi {speed(s.transfer.upload_speed)}{s.transfer.connection === 'connected' ? '' : ` · ${s.transfer.connection === 'firewalled' ? 'derrière un pare-feu' : 'déconnecté'}`}</span>{/if}
              {#if s.error}<span class="v-note rg-err" role="alert"><Icon name="alerte" size={14} /> {s.error}</span>{/if}
              {#each s.alerts as a (a.source + a.message)}
                <span class="v-note" class:rg-err={a.level === 'error'} class:rg-warn={a.level === 'warning'}><Icon name={a.level === 'notice' ? 'info' : 'alerte'} size={14} /> {a.message}</span>
              {/each}
            </span>
            <span class="v-tag {etat.tone}">{etat.label}</span>
          </li>
        {/each}
      </ul>
    </section>

    <section class="film-section">
      <h2 class="v-titre">Téléchargements bloqués</h2>
      {#if !status.queue.length}
        <p class="v-note">Aucun téléchargement bloqué dans Radarr, Sonarr ou Lidarr.</p>
      {:else}
        <ul class="rg-list">
          {#each status.queue as b (b.service + b.id)}
            <li class="rg-item">
              <span class="grow rg-body">
                <strong class="rg-titre">{b.title}</strong>
                <span class="v-note">{serviceName(b.service)}{b.state ? ` · ${b.state}` : ''}</span>
                {#each b.messages as m (m)}<span class="v-note" class:rg-err={b.level === 'error'} class:rg-warn={b.level === 'warning'}>{m}</span>{/each}
              </span>
              <span class="v-tag {b.level === 'error' ? 'dgr' : 'att'}">{b.level === 'error' ? 'En erreur' : 'Avertissement'}</span>
              <button class="plbls-button plbls-button--compact" disabled={busy === `q${b.service}${b.id}`} onclick={() => void remove(b, true)}>Autre version</button>
              <button class="plbls-button plbls-button--compact plbls-button--contour plbls-button--danger" disabled={busy === `q${b.service}${b.id}`} onclick={() => void remove(b, false)}>Retirer</button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="film-section">
      <h2 class="v-titre">Indexeurs</h2>
      {#if !indexers.length}
        <p class="v-note">Aucun indexeur annoncé par Prowlarr.</p>
      {:else}
        <ul class="rg-list">
          {#each indexers as i (i.id)}
            {@const etat = indexerState(i)}
            <li class="rg-item">
              <span class="grow rg-body">
                <strong>{i.name}</strong>
                <span class="v-note">{[i.protocol, i.privacy === 'private' ? 'privé' : i.privacy === 'public' ? 'public' : i.privacy === 'semiPrivate' ? 'semi-privé' : i.privacy].filter(Boolean).join(' · ')}</span>
                {#if i.enabled && i.failing}<span class="v-note rg-err">{i.last_failure ? `Dernier échec : ${date(i.last_failure)}` : 'En échec'}{i.disabled_until ? ` · suspendu jusqu’au ${date(i.disabled_until)}` : ''}</span>{/if}
              </span>
              <span class="v-tag {etat.tone}">{etat.label}</span>
              <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={busy === `i${i.id}`} onclick={() => void test(i)}>Tester</button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="film-section">
      <h2 class="v-titre">Disques</h2>
      {#if !status.disks.length}
        <p class="v-note">Aucun dossier racine annoncé.</p>
      {:else}
        <ul class="rg-list">
          {#each status.disks as d (d.service + d.path)}
            <li class="rg-item">
              <span class="grow rg-body"><strong class="v-mono rg-titre">{d.path}</strong><span class="v-note">{serviceName(d.service)}</span></span>
              {#if d.accessible}<span class="v-note">{d.free === null ? '' : `${bytes(d.free)} libres`}</span>{:else}<span class="v-tag dgr">Inaccessible</span>{/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<style>
  .rg-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .rg-item { display: flex; align-items: center; flex-wrap: wrap; gap: var(--espace-2); padding: var(--espace-2) var(--espace-3); border-radius: var(--r); background: var(--surface); box-shadow: var(--o1); }
  .rg-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .rg-body .v-note { display: inline-flex; align-items: center; gap: 4px; }
  .rg-titre { overflow-wrap: anywhere; }
  .rg-version { margin-left: var(--espace-2); font-weight: 400; }
  .rg-err { color: var(--danger); }
  .rg-warn { color: var(--attention); }
</style>
