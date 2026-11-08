<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from '../lib/api'
  import { app } from '../lib/store.svelte'
  import { prefs } from '../lib/prefs.svelte'
  import { notify } from '../lib/ui'
  import { music } from '../blocks/musique/music.svelte'
  import { clock, coverUrl } from '../blocks/musique/music'
  import { expectedPosition, lastAction } from '../blocks/regarder-ensemble/watch'
  import type { Cmd } from '../blocks/regarder-ensemble/watchsync'
  import type { MusicTrack } from '../lib/types'
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'

  let { channelId, serverId, people }: { channelId: string; serverId?: string; people: { user_id: string; pseudo: string }[] } = $props()

  const s = $derived(app.music[channelId]?.state)
  const meta = $derived(s?.meta)
  const canDrive = $derived(!!s && (!s.host_only || s.host === app.meId))
  const isHost = $derived(s?.host === app.meId)
  const fail = (e: unknown) => notify.error(e, 'Action impossible')
  const act = (a: string, b: { position?: number; rev?: number } = {}) => void api.watchAction(channelId, a, b, 'music').catch(fail)
  // les commandes de lecture passent par le moteur de synchronisation (comme « Regarder ensemble »), jamais directement par l'API
  const command = (c: Cmd) => (music.player ? music.player.command(c) : act(c.action, { position: c.position }))

  let now = $state(Date.now())
  onMount(() => { const t = setInterval(() => (now = Date.now()), 250); return () => clearInterval(t) })
  const total = $derived(meta?.duration || Math.round(music.player?.duration ?? 0))
  const pos = $derived(s ? Math.min(expectedPosition(s, app.music[channelId]?.offset ?? 0, now), total || Infinity) : 0)
  // pendant le glisser, la position visée est affichée sans être appliquée ; un clic ou la fin du glisser envoie un seul « se déplacer »
  let drag = $state<number | null>(null)

  // Le champ « Ajouter » prend un lien YouTube Music ou une recherche (résultats proposés après une courte pause de frappe).
  let link = $state('')
  let found = $state<MusicTrack[]>([])
  let searchError = $state('')
  const isLink = (v: string) => /^(https?:\/\/|music\.youtube\.com)/i.test(v)
  $effect(() => {
    const q = link.trim()
    searchError = ''
    if (q.length < 2 || isLink(q)) { found = []; return }
    let gone = false
    const t = setTimeout(() => {
      api.musicSearch(q).then((r) => { if (!gone) found = r }).catch((e) => { if (!gone) { found = []; searchError = e instanceof Error ? e.message : 'Recherche indisponible' } })
    }, 400)
    return () => { gone = true; clearTimeout(t) }
  })
  async function enqueue(v: string) {
    try { await api.watchEnqueue(channelId, v, 'music'); link = ''; found = [] } catch (err) { fail(err) }
  }
  function add(e: Event) {
    e.preventDefault()
    const v = link.trim()
    if (v && isLink(v)) void enqueue(v)
  }

  // glisser-déposer de la file (les boutons monter/descendre restent pour le clavier)
  let dragFrom = $state<number | null>(null)
  function drop(to: number) {
    const from = dragFrom
    dragFrom = null
    if (from === null || from === to) return
    const ids = s!.queue.map((q) => q.id)
    ids.splice(to, 0, ...ids.splice(from, 1))
    void api.watchReorder(channelId, ids, 'music').catch(fail)
  }
  const cycle = { '': 'one', one: 'all', all: '' } as const
  function move(i: number, d: number) {
    const ids = s!.queue.map((q) => q.id)
    ;[ids[i], ids[i + d]] = [ids[i + d], ids[i]]
    void api.watchReorder(channelId, ids, 'music').catch(fail)
  }
  let queueOpen = $state(false)
  const prevItem = $derived(s?.history[s.history.length - 1])
  const nextItem = $derived(s?.queue[0])
  // sens du glissement : vers l'arrière quand l'historique raccourcit (précédent)
  let lastHist = 0
  let lastDir = 1
  const dir = $derived.by(() => {
    const n = s?.history.length ?? 0
    lastDir = n < lastHist ? -1 : n > lastHist ? 1 : lastDir
    lastHist = n
    return lastDir
  })
  function volume(v: number) {
    prefs.musicVolume = v
    prefs.save()
    music.player?.setVolume(v)
  }
</script>

{#if s && meta}
  <section class="np" aria-label="En cours de lecture">
    <div class="np-top row">
      <ul class="np-people small" aria-label="Auditeurs">
        {#each people as p (p.user_id)}
          {@const st = s.ready[p.user_id]}
          <li title="{p.pseudo}{s.host === p.user_id ? ' (hôte)' : ''} : {st === false ? 'en chargement' : st ? 'prêt' : 'en attente'}">
            <span class="np-face"><Avatar pseudo={p.pseudo} userId={p.user_id} {serverId} /><span class="np-state" class:ready={st} class:loading={st === false} aria-hidden="true"></span></span>
            <span class="plbls-sr-only">{p.pseudo} : {st === false ? 'en chargement' : st ? 'prêt' : 'en attente'}</span>
          </li>
        {/each}
      </ul>
      <div class="np-bar row">
        <button class="ib" class:on={!!s.repeat} aria-pressed={!!s.repeat} disabled={!canDrive} onclick={() => void api.watchRepeat(channelId, cycle[s.repeat], 'music').catch(fail)}
          aria-label={s.repeat === 'one' ? 'Répéter le morceau' : s.repeat === 'all' ? 'Répéter la file' : 'Répétition désactivée'} title="Répétition : {s.repeat === 'one' ? 'le morceau' : s.repeat === 'all' ? 'la file' : 'non'}">
          <Icon name="repeat" size={20} />{#if s.repeat === 'one'}<span class="np-badge">1</span>{/if}
        </button>
        <button class="ib" disabled={!canDrive || s.queue.length < 2} onclick={() => act('shuffle')} aria-label="Mélanger la file" title="Mélanger la file"><Icon name="shuffle" size={20} /></button>
        <label class="np-vol row small" title="Volume de la musique (propre à toi)"><Icon name="volume" size={16} />
          <input type="range" min="0" max="100" value={prefs.musicVolume} oninput={(e) => volume(Number(e.currentTarget.value))} aria-label="Volume de la musique (propre à toi)" />
        </label>
        {#if isHost}
          <label class="small row"><input type="checkbox" checked={s.host_only} onchange={(e) => void api.watchHostOnly(channelId, e.currentTarget.checked, 'music').catch(fail)} /> Seul l'hôte pilote</label>
        {:else if s.host_only}<span class="small">Seul l'hôte pilote</span>{/if}
        {#if canDrive}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void api.watchStop(channelId, 'music').catch(fail)}>Arrêter la session</button>{/if}
      </div>
      <button class="plbls-button plbls-button--compact plbls-button--contour" aria-expanded={queueOpen} onclick={() => (queueOpen = !queueOpen)}>File d'attente ({s.queue.length})</button>
    </div>

    <div class="np-rail" style="--dir: {dir}">
        <div class="np-side">
        {#key s.video}{#if prevItem}
          <button class="np-side-cover" disabled={!canDrive} onclick={() => act('previous')} title="{prevItem.meta?.title ?? 'Précédent'}{prevItem.meta?.artist ? ' · ' + prevItem.meta.artist : ''}" aria-label="Morceau précédent : {prevItem.meta?.title ?? ''}">
            {#if prevItem.meta && coverUrl(prevItem.meta)}<img src={coverUrl(prevItem.meta)} alt="" />{/if}
          </button>
        {:else}<span class="np-side-cover np-empty" aria-hidden="true"></span>{/if}{/key}
        <span class="np-side-title truncate">{prevItem?.meta?.title ?? ''}</span>
        </div>
        <div class="np-slot" bind:this={music.slot} aria-label="Lecteur YouTube : {meta.title}"></div>
        <div class="np-side">
        {#key s.video}{#if nextItem}
          <button class="np-side-cover" disabled={!canDrive} onclick={() => act('ended', { rev: s.rev })} title="{nextItem.meta?.title ?? 'Suivant'}{nextItem.meta?.artist ? ' · ' + nextItem.meta.artist : ''}" aria-label="Morceau suivant : {nextItem.meta?.title ?? ''}">
            {#if nextItem.meta && coverUrl(nextItem.meta)}<img src={coverUrl(nextItem.meta)} alt="" />{/if}
          </button>
        {:else}<span class="np-side-cover np-empty" aria-hidden="true"></span>{/if}{/key}
        <span class="np-side-title truncate">{nextItem?.meta?.title ?? ''}</span>
        </div>
    </div>
    {#if music.player?.error}
      <div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{music.player.error}</p></div>
    {:else if music.player?.blocked}
      <button class="plbls-button np-unmute" onclick={() => music.player?.resume()}><Icon name="volume" size={18} /> Activer le son</button>
    {/if}
    <div class="np-info">
      <h2 class="truncate" title={meta.title}>{meta.title}</h2>
      <p class="truncate">{[meta.artist, meta.album].filter(Boolean).join(' · ')}</p>
    </div>

    <div class="np-progress row">
      <span class="small np-time">{clock(drag ?? pos) || '0:00'}</span>
      <input type="range" class="np-scrub grow" style="--p:{total ? Math.min(100, ((drag ?? pos) / total) * 100) : 0}%" min="0" max={total || 1} step="1" value={drag ?? pos} disabled={!canDrive || !total} aria-label="Position dans le morceau"
        oninput={(e) => (drag = Number(e.currentTarget.value))}
        onchange={(e) => { command({ action: 'seek', position: Number(e.currentTarget.value) }); drag = null }} />
      <span class="small np-time">{clock(total) || '0:00'}</span>
    </div>
    <div class="np-controls row">
      <button class="ib np-prev" disabled={!canDrive} onclick={() => act('previous')} aria-label="Précédent" title="Précédent"><Icon name="next" size={22} /></button>
      <button class="ib np-play" disabled={!canDrive} onclick={() => command({ action: s.playing ? 'pause' : 'play', position: pos })} aria-label={s.playing ? 'Pause' : 'Lecture'}><Icon name={s.playing ? 'pause' : 'play'} size={26} /></button>
      <button class="ib" disabled={!canDrive || (!s.queue.length && s.repeat !== 'all')} onclick={() => act('ended', { rev: s.rev })} aria-label="Suivant" title="Suivant"><Icon name="next" size={22} /></button>
    </div>
    <p class="small np-act" aria-live="polite">{music.player?.suspended ? 'Synchronisation suspendue quelques secondes : trop d’évènements.' : lastAction(s)}</p>

    {#if queueOpen}
      {#if s.queue.length}
        <ol class="np-queue np-full">
          {#each s.queue as q, i (q.id)}
            <li class="row" class:dragging={dragFrom === i} draggable={canDrive} ondragstart={() => (dragFrom = i)} ondragend={() => (dragFrom = null)}
              ondragover={(e) => { if (dragFrom !== null) e.preventDefault() }} ondrop={(e) => { e.preventDefault(); drop(i) }}>
              <span class="np-qc">{#if q.meta}<img src={coverUrl(q.meta)} alt="" loading="lazy" />{/if}</span>
              <span class="grow np-q"><span class="truncate">{q.meta?.title ?? q.video}</span><span class="small truncate">{q.meta?.artist ?? ''}{q.meta ? ' · ' : ''}{q.by}</span></span>
              {#if q.meta?.duration}<span class="small np-time">{clock(q.meta.duration)}</span>{/if}
              {#if canDrive}
                <button class="ib sm" disabled={i === 0} onclick={() => move(i, -1)} aria-label="Monter"><span class="up"><Icon name="chevron" size={14} /></span></button>
                <button class="ib sm" disabled={i === s.queue.length - 1} onclick={() => move(i, 1)} aria-label="Descendre"><span class="down"><Icon name="chevron" size={14} /></span></button>
              {/if}
              {#if canDrive || q.by_id === app.meId}<button class="ib sm" onclick={() => void api.watchDequeue(channelId, q.id, 'music').catch(fail)} aria-label="Retirer de la file"><Icon name="x" size={14} /></button>{/if}
            </li>
          {/each}
        </ol>
      {:else}
        <p class="small np-act">La file est vide : ajoutez un morceau, un album ou une playlist.</p>
      {/if}
    {/if}

    <div class="np-add">
      <form class="np-pill row" onsubmit={add}>
        <input class="plbls-input grow" bind:value={link} placeholder="https://… ou rechercher un morceau" aria-label="Rechercher un morceau ou coller un lien YouTube Music à ajouter à la file" maxlength="2000" />
        <button class="plbls-button plbls-button--compact" disabled={!isLink(link.trim())}>Ajouter</button>
      </form>
      {#if searchError}<p class="small np-act" role="alert">{searchError}</p>{/if}
      {#if found.length}
        <ul class="np-queue np-found" aria-label="Résultats de la recherche">
          {#each found as t (t.id)}
            <li><button class="row np-result" onclick={() => void enqueue(`https://music.youtube.com/watch?v=${t.id}`)} title="Ajouter à la file">
              <span class="np-qc"><img src={coverUrl(t)} alt="" loading="lazy" /></span>
              <span class="grow np-q"><span class="truncate">{t.title}</span><span class="small truncate">{t.artist}</span></span>
              {#if t.duration}<span class="small np-time">{clock(t.duration)}</span>{/if}
              <Icon name="plus" size={14} />
            </button></li>
          {/each}
        </ul>
      {/if}
    </div>
  </section>
{/if}
