<script lang="ts">
  import { app } from '../../lib/store.svelte'
  import { voice } from '../vocal/voice.svelte'
  import { WatchPlayer } from '../regarder-ensemble/watch.svelte'
  import { music } from './music.svelte'

  // Le lecteur officiel de YouTube reste visible (conditions d'utilisation) dans la vue « En cours de lecture », à l'emplacement `music.slot`.
  // Ailleurs il est hors écran : la musique continue, le mini-lecteur de la barre latérale suffit.
  const cid = $derived(voice.channelId && voice.state !== 'idle' && app.music[voice.channelId] ? voice.channelId : '')
  let host = $state<HTMLElement>()
  let rect = $state<{ top: number; left: number; width: number; height: number } | null>(null)

  $effect(() => {
    if (!cid || !host) return
    const el = host
    const p = new WatchPlayer(el, cid, 'music')
    music.player = p
    void p.start()
    return () => { p.destroy(); music.player = null; el.replaceChildren() }
  })

  // Sans cet appel le lecteur n'est jamais créé : chaque nouvel état de la session est appliqué au lecteur.
  $effect(() => {
    const s = cid ? app.music[cid]?.state : undefined
    if (s) { s.rev; s.video; music.player?.apply() }
  })

  $effect(() => {
    const slot = music.slot
    if (!slot || !cid) { rect = null; return }
    const update = () => {
      const r = slot.getBoundingClientRect()
      const view = slot.closest('.np')?.getBoundingClientRect()
      rect = view && (r.bottom < view.top || r.top > view.bottom) ? null : { top: r.top, left: r.left, width: r.width, height: r.height }
    }
    update()
    const ro = new ResizeObserver(update)
    ro.observe(slot)
    addEventListener('resize', update)
    addEventListener('scroll', update, true)
    return () => { ro.disconnect(); removeEventListener('resize', update); removeEventListener('scroll', update, true) }
  })
</script>

{#if cid}
  <div class="music-dock" class:docked={!!rect} bind:this={host} aria-label="Lecteur YouTube"
    style={rect ? `top:${rect.top}px;left:${rect.left}px;width:${rect.width}px;height:${rect.height}px` : ''}></div>
{/if}
