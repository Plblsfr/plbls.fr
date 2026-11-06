<script lang="ts">
  import BadgeIcon from './BadgeIcon.svelte'
  import type { Badge } from '../../lib/types'

  let { badges }: { badges: Badge[] } = $props()
</script>

<ul class="badge-row" aria-label="Badges">
  {#each badges as b (b.id)}
    <li>
      <button type="button" class="badge-btn" aria-label={b.name} aria-describedby="tip-{b.id}">
        <BadgeIcon badge={b} size={24} />
        <span role="tooltip" id="tip-{b.id}" class="badge-tip"><strong>{b.name}</strong>{#if b.description}<span>{b.description}</span>{/if}</span>
      </button>
    </li>
  {/each}
</ul>

<style>
  .badge-row { display: flex; flex-wrap: wrap; gap: var(--espace-2); margin: 0; padding: var(--espace-2) var(--espace-3); list-style: none; width: fit-content; max-width: 100%; border-radius: var(--rayon); background: var(--creme-creux); box-shadow: inset 0 0 0 1px var(--trait); }
  .badge-btn { position: relative; display: inline-flex; padding: 0; border: 0; background: none; border-radius: 6px; cursor: default; }
  .badge-btn:focus-visible { outline: 2px solid var(--rose-texte); outline-offset: 2px; }
  .badge-tip { position: absolute; z-index: var(--z-infobulle); bottom: calc(100% + 6px); left: 50%; transform: translateX(-50%); display: none; flex-direction: column; gap: 2px; width: max-content; max-width: min(240px, 70vw); padding: var(--espace-2) var(--espace-3); border-radius: var(--rayon); background: var(--noir); color: var(--creme); font-size: 12px; line-height: 1.3; text-align: left; white-space: normal; pointer-events: none; }
  .badge-btn:hover .badge-tip, .badge-btn:focus .badge-tip { display: flex; }
</style>
