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

  let { channelId }: { channelId: string } = $props()
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
  <div class="mini row" role="group" aria-label="En cours de lecture">
    <img src={coverUrl(s.meta)} alt="" />
    <span class="grow mini-t"><span class="truncate">{s.meta.title}</span><span class="small muted truncate">{s.meta.artist}</span></span>
    {#if canDrive}<button class="ib sm" onclick={toggle} aria-label={s.playing ? 'Pause' : 'Lecture'}><Icon name={s.playing ? 'pause' : 'play'} size={14} /></button>{/if}
  </div>
  <MiniProgress {pos} {total} canSeek={canDrive && !!music.player} onseek={seek} />
{/if}
