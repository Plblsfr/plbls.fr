<script lang="ts">
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { voice } from '../vocal/voice.svelte'
  import { solo } from './solo.svelte'
  import { notify } from '../../lib/ui'
  import { coverUrl } from '../musique/music'
  import { musicLink } from './videos'
  import Icon from '../../components/Icon.svelte'
  import MiniProgress from '../../components/MiniProgress.svelte'

  const t = $derived(solo.current)
  let volumeOpen = $state(false)
  const inVoice = $derived(voice.state !== 'idle' && !!voice.channelId)

  // « Partager dans le vocal » : le morceau en cours et la suite deviennent une session Music2gether, l'écoute seul s'arrête.
  async function share() {
    const cid = voice.channelId
    if (!cid || !t) return
    const links = [t, ...solo.upcoming].slice(0, 50).map((x) => musicLink(x.id))
    try {
      if (app.music[cid]) for (const l of links) await api.watchEnqueue(cid, l, 'music')
      else await api.watchStart(cid, links[0], links.slice(1), 'music')
      solo.stop()
    } catch (e) { notify.error(e, 'Partage impossible') }
  }
</script>

{#if t}
  <div class="solo" role="group" aria-label="Écoute personnelle">
    <div class="mini row">
      <img src={coverUrl(t)} alt="" />
      <span class="grow mini-t"><span class="truncate">{t.title}</span><span class="small muted truncate">{t.artist || 'Écoute personnelle'}</span></span>
      <button class="ib sm" onclick={() => solo.toggle()} aria-label={solo.playing ? 'Pause' : 'Lecture'}><Icon name={solo.playing ? 'pause' : 'play'} size={14} /></button>
      <button class="ib sm" onclick={() => solo.next()} aria-label="Suivant" title="Suivant"><Icon name="next" size={14} /></button>
      <button class="ib sm" onclick={() => solo.stop()} aria-label="Arrêter l'écoute" title="Arrêter"><Icon name="x" size={14} /></button>
    </div>
    {#if solo.blocked}<div class="mini row"><button class="plbls-button plbls-button--compact" onclick={() => { solo.blocked = false; solo.playing = true }}>Activer le son</button></div>{/if}
    <MiniProgress pos={solo.position} total={solo.duration} onseek={(s) => solo.seek(s)} />
    <div class="mini row">
      <button class="ib sm" onclick={() => (volumeOpen = !volumeOpen)} aria-expanded={volumeOpen} aria-label="Volume de l'écoute personnelle" title="Volume"><Icon name="volume" size={14} /></button>
      {#if volumeOpen}<input class="mini-volume" type="range" min="0" max="100" bind:value={solo.volume} aria-label="Volume de l'écoute personnelle" />{/if}
      {#if inVoice}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void share()}>Partager dans le vocal</button>{/if}
    </div>
    {#if solo.upcoming.length}<div class="small muted" style="padding: 0 8px 0 28px">À suivre : {solo.upcoming.length} titre{solo.upcoming.length > 1 ? 's' : ''}</div>{/if}
  </div>
{/if}
