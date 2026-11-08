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

<style>
  .mini-progress { gap: 6px; padding: 0 8px 0 28px; }
  .mini-time { flex: none; min-width: 3.2em; padding: 0; border: 0; background: none; color: inherit; font-family: var(--f-mono); font-size: 11px; font-variant-numeric: tabular-nums; }
  .mini-time:first-child { text-align: left; }
  button.mini-time { text-align: right; cursor: pointer; }
  button.mini-time:disabled { cursor: default; }
  button.mini-time:focus-visible { outline: none; box-shadow: var(--anneau); border-radius: 4px; }
  .mini-bar { height: 14px; min-width: 0; background: none; cursor: pointer; appearance: none; -webkit-appearance: none; }
  .mini-bar:disabled { cursor: default; opacity: 0.6; }
  .mini-bar:focus-visible { outline: none; box-shadow: var(--anneau); border-radius: 4px; }
  .mini-bar::-webkit-slider-runnable-track { height: 3px; border-radius: 2px; background: linear-gradient(to right, var(--rose-fonce) var(--p), color-mix(in srgb, currentColor 20%, transparent) var(--p)); }
  .mini-bar::-moz-range-track { height: 3px; border-radius: 2px; background: linear-gradient(to right, var(--rose-fonce) var(--p), color-mix(in srgb, currentColor 20%, transparent) var(--p)); }
  .mini-bar::-webkit-slider-thumb { width: 8px; height: 8px; margin-top: -2.5px; border-radius: 50%; background: currentColor; transition: transform 0.1s; -webkit-appearance: none; }
  .mini-bar::-moz-range-thumb { width: 8px; height: 8px; border: 0; border-radius: 50%; background: currentColor; transition: transform 0.1s; }
  .mini-bar:hover::-webkit-slider-thumb, .mini-bar:active::-webkit-slider-thumb { transform: scale(1.6); }
  .mini-bar:hover::-moz-range-thumb, .mini-bar:active::-moz-range-thumb { transform: scale(1.6); }
</style>
