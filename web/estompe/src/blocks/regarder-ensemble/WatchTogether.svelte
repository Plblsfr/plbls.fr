<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { WatchPlayer, watchView } from './watch.svelte'
  import { lastAction } from './watch'
  import VoiceControls from '../vocal/VoiceControls.svelte'
  import Avatar from '../../components/Avatar.svelte'
  import Icon from '../../components/Icon.svelte'
  import { PROVIDER_LABEL } from './providers'
  import { trackName } from '../films/films'
  import { hrefs } from '../../lib/route.svelte'
  import type { SessionKind } from '../../lib/types'

  let { channelId, people, kind = 'watch', theater = false, ontheater }: { channelId: string; people: { user_id: string; pseudo: string }[]; kind?: SessionKind; theater?: boolean; ontheater?: () => void } = $props()

  const cur = $derived(app[kind][channelId])
  const s = $derived(cur?.state)
  const canDrive = $derived(!!s && (!s.host_only || s.host === app.meId))
  const isHost = $derived(s?.host === app.meId)
  const action = $derived(s ? lastAction(s) : '')
  const fail = (e: unknown) => notify.error(e, 'Action impossible')

  let host = $state<HTMLElement>()
  let player = $state<WatchPlayer>()
  let link = $state('')
  let drag = $state<number | null>(null)
  let idle = $state(false)
  let idleTimer: ReturnType<typeof setTimeout> | undefined
  const wake = () => { idle = false; clearTimeout(idleTimer); idleTimer = setTimeout(() => (idle = !!s?.playing && !host?.closest('.watch-player')?.matches(':focus-within')), 2500) }
  let vol = $state(100)
  let muted = $state(false)
  const RATES = [0.5, 0.75, 1, 1.25, 1.5, 2]
  const clock = (t: number) => `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, '0')}`

  onMount(() => {
    const p = new WatchPlayer(host!.appendChild(document.createElement('div')), channelId, kind)
    player = p
    watchView.player = p
    void p.start()
    wake()
    return () => { p.destroy(); watchView.player = null; clearTimeout(idleTimer) }
  })
  $effect(() => { if (s) { s.rev; s.video; player?.apply() } })
  $effect(() => { s?.provider; s?.video; player?.setVolume(muted ? 0 : vol) })
  const pct = (v: number, max: number) => `${max ? Math.min(100, (v / max) * 100) : 0}%`
  const stop = () => void api.watchAction(channelId, 'ended', { rev: s!.rev }, kind).catch(fail)
  const toggle = () => player?.command({ action: s!.playing ? 'pause' : 'play', position: player.position })

  async function add(e: Event) {
    e.preventDefault()
    const v = link.trim()
    if (!v) return
    try { await api.watchEnqueue(channelId, v, kind); link = '' } catch (err) { fail(err) }
  }
  function move(i: number, d: number) {
    const ids = s!.queue.map((q) => q.id)
    ;[ids[i], ids[i + d]] = [ids[i + d], ids[i]]
    void api.watchReorder(channelId, ids, kind).catch(fail)
  }
</script>

{#if s}
  {@const live = s.provider === 'twitch_live' || s.provider === 'twitch_clip'}
  {@const queued = s.queue.length}
  <section class="watch" aria-label={kind === 'cinema' ? 'Cinéma' : 'Regarder ensemble'}>
    <div class="watch-main">
      <div class="watch-head row">
        <span class="watch-pill" class:att={player?.suspended} aria-live="polite"><Icon name={player?.suspended ? 'alerte' : 'ok'} size={14} />{player?.suspended ? 'Synchronisation suspendue quelques secondes : trop d’évènements.' : action || 'Lecture synchronisée'}</span>
        <span class="grow"></span>
        {#if isHost && s.provider !== 'film'}
          <label class="plbls-checkbox watch-host"><input class="plbls-checkbox__input" type="checkbox" checked={s.host_only} onchange={(e) => void api.watchHostOnly(channelId, e.currentTarget.checked, kind).catch(fail)} /><span class="plbls-checkbox__case"></span><span class="plbls-checkbox__texte">Seul l'hôte pilote</span></label>
        {:else if s.host_only}<span class="v-tag"><Icon name="cadenas" size={12} /> Seul l'hôte pilote</span>{/if}
        {#if canDrive && queued}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={stop}><Icon name="suivant" size={14} /> {s.provider === 'film' ? 'Épisode suivant' : 'Suivante'}</button>{/if}
        {#if canDrive}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void api.watchStop(channelId, kind).catch(fail)}>Arrêter</button>{/if}
      </div>
      <div class="watch-player" class:idle onpointermove={wake} onpointerdown={wake} onfocusin={wake} role="presentation">
        {#if theater}
          <div class="watch-top">
            <button class="watch-chip" onclick={ontheater}><Icon name="reduire" size={18} /> Quitter le mode cinéma</button>
            <span class="watch-faces">{#each people as p (p.user_id)}<Avatar pseudo={p.pseudo} userId={p.user_id} size="sm" />{/each}</span>
            <span class="watch-chip">File {queued} / 10</span>
          </div>
        {/if}
        <div class="watch-video" bind:this={host}></div>
        {#if player?.error}
          <div class="watch-msg plbls-alert plbls-alert--erreur" role="alert">
            <p class="plbls-alert__corps">{player.error}</p>
            {#if queued || canDrive}<button class="plbls-button plbls-button--compact" onclick={stop}>Passer à la suivante</button>{/if}
          </div>
        {:else if player?.blocked}
          <button class="watch-msg plbls-button v-rose" onclick={() => player?.resume()}><Icon name="lecture" size={18} /> Lancer la lecture</button>
        {:else if player && !live && !s.playing && canDrive}
          <button class="watch-big" onclick={toggle} aria-label="Lecture"><Icon name="lecture" size={34} /></button>
        {/if}
        {#if player && !live}
          <div class="watch-bar" role="group" aria-label="Lecture">
            <button class="watch-btn" disabled={!canDrive} onclick={toggle} aria-label={s.playing ? 'Pause' : 'Lecture'}><Icon name={s.playing ? 'pause' : 'lecture'} size={22} /></button>
            <span class="watch-time v-mono">{clock(drag ?? player.position)}{player.duration ? ` / ${clock(player.duration)}` : ''}</span>
            <input class="watch-range watch-seek grow" style="--p:{pct(drag ?? player.position, player.duration)}" type="range" min="0" max={Math.max(player.duration, 1)} step="1" disabled={!canDrive || !player.duration} value={drag ?? player.position} aria-label="Position dans la vidéo"
              oninput={(e) => (drag = Number(e.currentTarget.value))}
              onchange={(e) => { player?.command({ action: 'seek', position: Number(e.currentTarget.value) }); drag = null }} />
            <div class="watch-bar-title grow"><strong class="truncate">{s.meta?.title ?? PROVIDER_LABEL[s.provider] ?? 'Vidéo'}</strong><span class="watch-by">ajouté par {s.by}</span></div>
            <button class="watch-btn" onclick={() => (muted = !muted)} aria-label={muted ? 'Rétablir le son' : 'Couper le son'} aria-pressed={muted}><Icon name={muted ? 'casqueoff' : 'volmax'} size={22} /></button>
            <input class="watch-range watch-vol" style="--p:{muted ? 0 : vol}%" type="range" min="0" max="100" value={muted ? 0 : vol} aria-label="Volume" oninput={(e) => { vol = Number(e.currentTarget.value); muted = vol === 0 }} />
            {#if player.tracks}
              {@const tracks = player.tracks}
              {#if tracks.audio.length > 1}
                <select class="watch-rate" value={tracks.audioN} onchange={(e) => player?.setAudio(Number(e.currentTarget.value))} aria-label="Piste audio">
                  {#each tracks.audio as a (a.n)}<option value={a.n}>{trackName(a)}</option>{/each}
                </select>
              {/if}
              {#if tracks.subtitles.length}
                <select class="watch-rate" value={tracks.subN ?? -1} onchange={(e) => { const n = Number(e.currentTarget.value); player?.setSubtitle(n < 0 ? null : n) }} aria-label="Sous-titres">
                  <option value={-1}>Sans sous-titres</option>
                  {#each tracks.subtitles as t (t.n)}<option value={t.n}>{trackName(t)}</option>{/each}
                </select>
              {/if}
            {/if}
            <select class="watch-rate" disabled={!canDrive} value={s.rate} onchange={(e) => player?.command({ action: 'rate', rate: Number(e.currentTarget.value) })} aria-label="Vitesse">
              {#each RATES as r (r)}<option value={r}>×{r}</option>{/each}
            </select>
            {#if theater}<div class="watch-voice"><VoiceControls /></div>{/if}
          </div>
        {/if}
      </div>
      <div class="watch-title row">
        <div class="grow">
          <h2>{s.meta?.title ?? PROVIDER_LABEL[s.provider] ?? 'Vidéo'}</h2>
          <p class="v-note">{s.meta?.title ? `${PROVIDER_LABEL[s.provider] ?? 'Vidéo'} · ` : ''}ajouté par {s.by}</p>
        </div>
        {#if ontheater}<button class="plbls-button plbls-button--contour" onclick={ontheater}><Icon name="film" size={18} /> Mode cinéma</button>{/if}
      </div>
    </div>
    <aside class="watch-side" aria-label="Participants et file d'attente">
      <section class="watch-card">
      <h3 class="watch-h">Sur scène <span class="watch-count">{people.length}</span></h3>
      <ul class="watch-people" aria-label="Participants">
        {#each people as p (p.user_id)}
          <li class:loading={s.ready[p.user_id] === false}><Avatar pseudo={p.pseudo} userId={p.user_id} size="sm" /><span class="grow truncate">{p.pseudo}{s.host === p.user_id ? ' (hôte)' : ''}</span><span class="v-tag" class:ok={!!s.ready[p.user_id]} class:att={s.ready[p.user_id] === false}>{s.ready[p.user_id] === false ? 'chargement' : s.ready[p.user_id] ? 'prêt' : '…'}</span></li>
        {/each}
      </ul>
      </section>
      <section class="watch-card">
      <h3 class="watch-h">File d'attente <span class="watch-count v-mono">{queued} / 10</span></h3>
      {#if kind === 'cinema'}
        {#if canDrive}<a class="plbls-button plbls-button--compact plbls-button--contour" href={hrefs.films()}><Icon name="film" size={16} /> Changer de titre</a>{/if}
      {:else}
      <form class="watch-add" onsubmit={add}>
        <input class="watch-link" bind:value={link} placeholder="Lien ou fichier à ajouter" aria-label="Lien de la vidéo à ajouter à la file" maxlength="2000" />
        <button class="watch-btn watch-go" disabled={!link.trim()} aria-label="Ajouter"><Icon name="plus" size={18} /></button>
      </form>
      <p class="v-mono v-mute watch-srcs">YouTube · Twitch · Vimeo · Dailymotion · fichier</p>
      {/if}
      <ol class="watch-queue">
        <li class="row now">
          <Icon name="lecture" size={14} /><span class="grow truncate small"><strong class="watch-now">En lecture</strong> · {s.meta?.title ?? (s.provider === 'file' ? s.video.replace(/^https?:\/\//, '') : PROVIDER_LABEL[s.provider] ?? 'Vidéo')}</span>
        </li>
        {#each s.queue as q, i (q.id)}
          <li class="row">
            {#if q.provider === 'youtube'}<img src="https://img.youtube.com/vi/{q.video}/default.jpg" alt="" loading="lazy" referrerpolicy="no-referrer" />{/if}
            <span class="grow truncate small">{#if q.provider === 'film'}{q.meta?.title ?? 'Épisode'}{:else}{PROVIDER_LABEL[q.provider] ?? 'Vidéo'} · {q.provider === 'file' ? q.video.replace(/^https?:\/\//, '') : q.video} · {q.by}{/if}</span>
            {#if canDrive}
              <button class="v-ib sm" disabled={i === 0} onclick={() => move(i, -1)} aria-label="Monter"><Icon name="envoi" size={14} /></button>
              <button class="v-ib sm" disabled={i === s.queue.length - 1} onclick={() => move(i, 1)} aria-label="Descendre"><Icon name="bas" size={14} /></button>
            {/if}
            {#if canDrive || q.by_id === app.meId}<button class="v-ib sm" onclick={() => void api.watchDequeue(channelId, q.id, kind).catch(fail)} aria-label="Retirer de la file"><Icon name="x" size={14} /></button>{/if}
          </li>
        {/each}
      </ol>
      </section>
    </aside>
  </section>
{/if}
