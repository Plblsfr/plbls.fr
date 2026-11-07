<script lang="ts">
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { voice } from '../vocal/voice.svelte'
  import { onMount } from 'svelte'
  import { music } from './music.svelte'
  import { notify } from '../../lib/ui'
  import { coverUrl } from './music'
  import { expectedPosition } from '../regarder-ensemble/watch'
  import Icon from '../../components/Icon.svelte'
  import MiniProgress from '../../components/MiniProgress.svelte'

  let { channelId, salon = false }: { channelId: string; salon?: boolean } = $props()
  const where = $derived(salon ? (app.channel(channelId)?.channel.name ?? '') : '')
  const cur = $derived(app.music[channelId])
  const s = $derived(cur?.state)
  const canDrive = $derived(voice.channelId === channelId && voice.state !== 'idle' && !!s && (!s.host_only || s.host === app.meId))
  let now = $state(Date.now())
  onMount(() => { const t = setInterval(() => (now = Date.now()), 250); return () => clearInterval(t) })
  const total = $derived(s?.meta?.duration || Math.round(music.player?.duration ?? 0))
  const pos = $derived(s ? expectedPosition(s, cur!.offset, now) : 0)
  const seek = (p: number) => music.player?.command({ action: 'seek', position: p })
  const toggle = () => void api.watchAction(channelId, s!.playing ? 'pause' : 'play', { position: expectedPosition(s!, cur!.offset) }, 'music').catch((e) => notify.error(e, 'Action impossible'))
</script>

{#if s?.meta}
  <div class="lect" role="group" aria-label="En cours de lecture">
    <div class="lect-l">
      {#if coverUrl(s.meta)}<img class="lect-pochette" src={coverUrl(s.meta)} alt="" />{:else}<span class="lect-pochette" aria-hidden="true"><Icon name="note" size={16} /></span>{/if}
      <span class="lect-t"><span class="lect-titre">{s.meta.title}</span><span class="lect-sous">{s.meta.artist}{#if where} · Écoute ensemble · {where}{/if}</span></span>
      {#if canDrive}<button class="ib lect-ib" onclick={toggle} aria-label={s.playing ? 'Pause' : 'Lecture'}><Icon name={s.playing ? 'pause' : 'play'} size={16} /></button>{/if}
    </div>
    <MiniProgress {pos} {total} canSeek={canDrive && !!music.player} onseek={seek} />
  </div>
{/if}

<style>
  .lect { display: grid; gap: 2px; min-width: 0; font-size: 13px; }
  .lect-l { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .lect-pochette { flex: none; display: grid; place-items: center; width: 36px; height: 36px; border-radius: var(--r-s); object-fit: cover; background: color-mix(in srgb, currentColor 14%, transparent); }
  .lect-t { display: grid; flex: 1; min-width: 0; line-height: 1.25; }
  .lect-titre { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lect-sous { font-size: 11.5px; opacity: 0.72; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lect-ib { color: inherit; }
  .lect :global(.mini-progress) { padding: 0; gap: 6px; }
  .lect :global(.mini-time) { color: inherit; opacity: 0.72; font-family: var(--f-mono); font-size: 10.5px; }
  .lect :global(.mini-bar::-webkit-slider-runnable-track) { background: linear-gradient(to right, var(--rose-fonce) var(--p), color-mix(in srgb, currentColor 20%, transparent) var(--p)); }
  .lect :global(.mini-bar::-moz-range-track) { background: linear-gradient(to right, var(--rose-fonce) var(--p), color-mix(in srgb, currentColor 20%, transparent) var(--p)); }
  .lect :global(.mini-bar::-webkit-slider-thumb) { background: currentColor; }
  .lect :global(.mini-bar::-moz-range-thumb) { background: currentColor; }
</style>
