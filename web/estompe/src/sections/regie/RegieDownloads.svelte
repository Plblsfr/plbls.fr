<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { confirmer } from '../../lib/confirm.svelte'
  import { menuFrom } from '../../lib/ctxmenu.svelte'
  import { notify } from '../../lib/ui'
  import RegieLimits from './RegieLimits.svelte'
  import RegieTorrent from './RegieTorrent.svelte'
  import { Suivi } from './regie.svelte'
  import { GROUPS, bytes, eta, seedingTime, filterTorrents, groupCounts, isStopped, percent, regieApi, speed, torrentLabel, torrentState, type Torrent, type TorrentGroup } from './regie'

  const suivi = new Suivi()
  $effect(() => { suivi.start(); return () => suivi.stop() })

  let group = $state<TorrentGroup | ''>('')
  let query = $state('')
  let busy = $state('')
  let opened = $state('')
  let limits = $state(false)

  const all = $derived(suivi.data?.torrents ?? [])
  const counts = $derived(groupCounts(all))
  const shown = $derived(filterTorrents(all, group, query))
  const transfer = $derived(suivi.data?.transfer)
  const openedTorrent = $derived(all.find((t) => t.hash === opened))

  async function run(t: Torrent, action: () => Promise<void>, echec: string) {
    busy = t.hash
    try { await action() } catch (e) { notify.error(e, echec) } finally { busy = '' }
  }

  async function remove(t: Torrent, files: boolean) {
    const ok = await confirmer({
      titre: `Supprimer « ${torrentLabel(t).title} » ?`,
      message: files ? 'Le torrent est retiré de qBittorrent et ses fichiers sont effacés du disque.' : 'Le torrent est retiré de qBittorrent. Ses fichiers restent sur le disque.',
      action: files ? 'Supprimer avec les fichiers' : 'Supprimer',
      destructif: true,
    })
    if (ok) await run(t, () => regieApi.remove([t.hash], files), 'Le torrent n’a pas pu être supprimé')
  }

  function menu(el: HTMLElement, t: Torrent) {
    menuFrom(el, [
      { label: 'Détails et fichiers', run: () => (opened = t.hash) },
      { label: 'Revérifier les fichiers', run: () => void run(t, () => regieApi.recheck([t.hash]), 'La vérification n’a pas pu être lancée') },
      { label: 'Supprimer, garder les fichiers', sep: true, danger: true, run: () => void remove(t, false) },
      { label: 'Supprimer avec les fichiers', danger: true, run: () => void remove(t, true) },
    ])
  }
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Téléchargements</h2>
    {#if transfer}
      <span class="v-note rg-debits"><Icon name="down" size={14} /> {speed(transfer.download_speed)} <Icon name="up" size={14} /> {speed(transfer.upload_speed)}{#if transfer.free_space !== null} · {bytes(transfer.free_space)} libres{/if}</span>
      {#if transfer.alternative}<span class="v-tag att">Limites alternatives</span>{/if}
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (limits = true)}><Icon name="curseurs" size={14} /> Limites</button>
    {/if}
  </div>

  {#if suivi.data?.error || suivi.failed}
    <p class="v-well v-note rg-err" role="alert"><Icon name="alerte" size={14} /> {suivi.data?.error ? `qBittorrent : ${suivi.data.error}` : `Suivi interrompu : ${suivi.failed}`}</p>
  {/if}

  {#if !suivi.data}
    {#if !suivi.failed}<Loader />{/if}
  {:else if suivi.data.torrents}
    <div class="film-filters">
      <div class="v-seg rg-seg" role="group" aria-label="État">
        <button aria-pressed={group === ''} onclick={() => (group = '')}>Tous <span class="v-mono">{all.length}</span></button>
        {#each GROUPS.filter((g) => counts[g.id] || group === g.id) as g (g.id)}
          <button aria-pressed={group === g.id} onclick={() => (group = g.id)}>{g.label} <span class="v-mono">{counts[g.id]}</span></button>
        {/each}
      </div>
      <span class="film-search"><Icon name="cherche" size={16} /><input class="plbls-input" type="search" aria-label="Filtrer les téléchargements" placeholder="Titre, catégorie, tag" maxlength="80" bind:value={query} /></span>
    </div>

    {#if !shown.length}
      <div class="v-empty"><Icon name="ok" size={28} /><p class="v-note">{all.length ? 'Aucun téléchargement ne correspond.' : 'Aucun téléchargement dans qBittorrent.'}</p></div>
    {:else}
      <ul class="rg-list">
        {#each shown as t (t.hash)}
          {@const etat = torrentState(t.state)}
          {@const nom = torrentLabel(t)}
          {@const reste = eta(t.eta)}
          {@const partage = seedingTime(t.seeding_time)}
          <li class="rg-item rg-torrent">
            <button class="rg-ouvrir grow" onclick={() => (opened = t.hash)} aria-label="Détails de {nom.title}">
              <strong class="rg-titre">{nom.title}</strong>
              {#if nom.sub}<span class="v-note rg-titre">{nom.sub}</span>{/if}
              <span class="rg-barre" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.floor(t.progress * 100)} aria-label="Progression"><span class:fini={t.progress >= 1} style:width="{Math.min(t.progress, 1) * 100}%"></span></span>
              <span class="v-note">
                {percent(t.progress)} de {bytes(t.size)}
                {#if etat.group === 'downloading' && t.download_speed} · {speed(t.download_speed)}{/if}
                {#if t.upload_speed} · envoi {speed(t.upload_speed)}{/if}
                {#if reste && etat.group === 'downloading'} · reste {reste}{/if}
                {#if t.progress >= 1} · ratio {t.ratio.toLocaleString('fr-FR', { maximumFractionDigits: 2 })}{/if}
                {#if partage} · partagé {partage}{/if}
                {#if etat.group === 'downloading'} · {t.seeds} source{t.seeds > 1 ? 's' : ''}{/if}
              </span>
            </button>
            {#if t.category}<span class="v-tag">{t.category}</span>{/if}
            <span class="v-tag {etat.tone}">{etat.label}</span>
            {#if isStopped(t)}
              <button class="ib" disabled={busy === t.hash} onclick={() => void run(t, () => regieApi.resume([t.hash]), 'Le torrent n’a pas pu être repris')} aria-label="Reprendre {nom.title}" title="Reprendre"><Icon name="lecture" /></button>
            {:else}
              <button class="ib" disabled={busy === t.hash} onclick={() => void run(t, () => regieApi.pause([t.hash]), 'Le torrent n’a pas pu être suspendu')} aria-label="Suspendre {nom.title}" title="Suspendre"><Icon name="pause" /></button>
            {/if}
            <button class="ib" onclick={(e) => menu(e.currentTarget, t)} aria-haspopup="menu" aria-label="Actions sur {nom.title}" title="Actions"><Icon name="plus3" /></button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>
{#if openedTorrent}<RegieTorrent torrent={openedTorrent} categories={suivi.data?.categories ?? []} onclose={() => (opened = '')} />{/if}
{#if limits && transfer}<RegieLimits {transfer} onclose={() => (limits = false)} />{/if}

<style>
  .rg-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .rg-item { display: flex; align-items: center; flex-wrap: wrap; gap: var(--espace-2); padding: var(--espace-2) var(--espace-3); border-radius: var(--r); background: var(--surface); box-shadow: var(--o1); }
  .rg-ouvrir { display: flex; flex-direction: column; gap: 4px; min-width: min(100%, 260px); padding: 0; border: 0; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .rg-ouvrir:focus-visible { outline: none; box-shadow: var(--anneau-in); border-radius: var(--r-s); }
  .rg-titre { overflow-wrap: anywhere; }
  .rg-barre { display: block; height: 6px; border-radius: 3px; background: var(--papier-3); overflow: hidden; }
  .rg-barre > span { display: block; height: 100%; border-radius: 3px; background: var(--info); }
  .rg-barre > span.fini { background: var(--ok); }
  .rg-debits { display: inline-flex; align-items: center; gap: 4px; }
  .rg-seg { flex-wrap: wrap; }
  .rg-seg .v-mono { margin-left: 6px; opacity: 0.7; }
  .rg-err { display: flex; align-items: center; gap: 6px; margin: 0; color: var(--danger); }
</style>
