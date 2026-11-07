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
  <div class="lect" role="group" aria-label="Écoute personnelle">
    <div class="lect-l">
      {#if coverUrl(t)}<img class="lect-pochette" src={coverUrl(t)} alt="" />{:else}<span class="lect-pochette" aria-hidden="true"><Icon name="casque" size={16} /></span>{/if}
      <span class="lect-t"><span class="lect-titre">{t.title}</span><span class="lect-sous">{t.artist || 'Écoute personnelle'}{#if solo.upcoming.length} · À suivre : {solo.upcoming.length} titre{solo.upcoming.length > 1 ? 's' : ''}{/if}</span></span>
      <button class="ib lect-ib" onclick={() => solo.toggle()} aria-label={solo.playing ? 'Pause' : 'Lecture'}><Icon name={solo.playing ? 'pause' : 'play'} size={16} /></button>
      <button class="ib lect-ib" onclick={() => solo.next()} aria-label="Suivant" title="Suivant"><Icon name="next" size={16} /></button>
      <button class="ib lect-ib" onclick={() => solo.stop()} aria-label="Arrêter l'écoute" title="Arrêter"><Icon name="x" size={16} /></button>
    </div>
    {#if solo.blocked}<button class="v-btn sm rose" onclick={() => { solo.blocked = false; solo.playing = true }}>Activer le son</button>{/if}
    <MiniProgress pos={solo.position} total={solo.duration} onseek={(s) => solo.seek(s)} />
    <div class="lect-l">
      {#if inVoice}<button class="v-btn sm rose grow" onclick={() => void share()}><Icon name="gens" size={15} />Partager dans le vocal</button>{:else}<span class="grow"></span>{/if}
      {#if volumeOpen}<input class="lect-volume" type="range" min="0" max="100" bind:value={solo.volume} aria-label="Volume de l'écoute personnelle" />{/if}
      <button class="ib lect-ib" onclick={() => (volumeOpen = !volumeOpen)} aria-expanded={volumeOpen} aria-label="Volume de l'écoute personnelle" title="Volume"><Icon name="volmax" size={16} /></button>
    </div>
  </div>
{/if}

<style>
  .lect { display: grid; gap: 4px; min-width: 0; font-size: 13px; }
  .lect-l { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .lect-pochette { flex: none; display: grid; place-items: center; width: 36px; height: 36px; margin-right: 4px; border-radius: var(--r-s); object-fit: cover; background: color-mix(in srgb, currentColor 14%, transparent); }
  .lect-t { display: grid; flex: 1; min-width: 0; line-height: 1.25; }
  .lect-titre { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lect-sous { font-size: 11.5px; opacity: 0.72; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lect-ib { width: 32px; height: 32px; color: inherit; }
  .lect .v-btn.sm { height: 30px; }
  .lect-volume { width: 0; flex: 1 1 80px; min-width: 60px; accent-color: var(--rose-fonce); }
  .lect :global(.mini-progress) { padding: 0; gap: 6px; }
  .lect :global(.mini-time) { color: inherit; opacity: 0.72; font-family: var(--f-mono); font-size: 10.5px; }
  .lect :global(.mini-bar::-webkit-slider-runnable-track) { background: linear-gradient(to right, var(--rose-fonce) var(--p), color-mix(in srgb, currentColor 20%, transparent) var(--p)); }
  .lect :global(.mini-bar::-moz-range-track) { background: linear-gradient(to right, var(--rose-fonce) var(--p), color-mix(in srgb, currentColor 20%, transparent) var(--p)); }
  .lect :global(.mini-bar::-webkit-slider-thumb) { background: currentColor; }
  .lect :global(.mini-bar::-moz-range-thumb) { background: currentColor; }
  @media (pointer: coarse) { .lect-ib { width: 44px; height: 44px; } .lect .v-btn.sm { min-height: 44px; } }
</style>
