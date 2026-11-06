<script lang="ts">
  import { providers, type Adapter } from '../regarder-ensemble/providers'
  import { solo } from './solo.svelte'
  import { notify } from '../../lib/ui'

  // Écoute personnelle : lecteur officiel YouTube, monté tant qu'un morceau est en cours. Comme celui de Music2gether (`MusicDock`), il reste hors écran
  // quand on navigue : le mini-lecteur de la barre latérale (pochette, titre, commandes) en est la face visible.
  let host = $state<HTMLElement>()
  let player = $state<Adapter | null>(null)
  const active = $derived(!!solo.current)

  $effect(() => {
    if (!active || !host) return
    const el = host
    let gone = false
    let made: Adapter | null = null
    void providers.youtube(el, {
      onState: (s) => { if (s === 'ended') solo.next(); else if (s === 'playing') { solo.playing = true; solo.blocked = false } else if (s === 'paused') solo.playing = false },
      onBlocked: () => { solo.blocked = true },
      onError: (m) => { notify.error(new Error(m), 'Lecture impossible'); solo.next() },
    }).then((a) => { if (gone) a.destroy(); else { made = a; player = a } }).catch((e) => { notify.error(e, 'Lecteur YouTube indisponible'); solo.stop() })
    return () => { gone = true; made?.destroy(); player = null; el.replaceChildren() }
  })

  // Valeurs primitives dérivées : ajouter un morceau à la file ne relance ni ne met en pause le morceau en cours.
  const id = $derived(solo.current?.id)
  $effect(() => {
    solo.position = 0
    solo.duration = 0
    if (player && id) player.load(id, 0, true)
  })
  $effect(() => {
    const p = player
    if (!p) return
    solo.seekTo = (s) => p.seek(s)
    const t = setInterval(() => { solo.position = p.time(); solo.duration = p.duration?.() ?? 0 }, 500)
    return () => { clearInterval(t); solo.seekTo = null }
  })
  $effect(() => { if (player && id) (solo.playing ? player.play() : player.pause()) })
  $effect(() => { player?.setVolume?.(solo.volume) })
</script>

{#if active}<div class="music-dock" data-solo bind:this={host} aria-label="Lecteur YouTube (écoute personnelle)"></div>{/if}
