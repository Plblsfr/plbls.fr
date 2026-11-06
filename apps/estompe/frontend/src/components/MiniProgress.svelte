<script lang="ts">
  import { clock } from '../blocks/musique/music'

  let { pos, total, canSeek = true, onseek }: { pos: number; total: number; canSeek?: boolean; onseek: (s: number) => void } = $props()

  // pendant le glisser, la position visée est affichée sans être appliquée ; un seul « se déplacer » à la fin
  let drag = $state<number | null>(null)
  let remaining = $state(false)
  const shown = $derived(Math.min(drag ?? pos, total || Infinity))
  const end = $derived(!total ? '--:--' : remaining ? `-${clock(total - shown) || '0:00'}` : clock(total))
</script>

<div class="mini-progress row">
  <span class="small muted mini-time" title={drag !== null ? 'Position visée' : undefined}>{clock(shown) || '0:00'}</span>
  <input type="range" class="grow mini-bar" min="0" max={total || 1} step="1" value={shown} style="--p:{total ? (shown / total) * 100 : 0}%"
    disabled={!canSeek || !total} aria-label="Position dans le morceau"
    oninput={(e) => (drag = Number(e.currentTarget.value))}
    onchange={(e) => { onseek(Number(e.currentTarget.value)); drag = null }} />
  <button class="small muted mini-time" disabled={!total} onclick={() => (remaining = !remaining)} aria-label={remaining ? 'Afficher la durée' : 'Afficher le temps restant'}>{end}</button>
</div>
