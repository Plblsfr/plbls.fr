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
  .badge-row { display: flex; flex-wrap: wrap; gap: 6px; width: fit-content; max-width: 100%; margin: 0; padding: 8px 10px; list-style: none; border-radius: var(--r); background: var(--papier-2); }
  .badge-btn { position: relative; display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; padding: 0; border: 0; border-radius: var(--r-xs); background: none; cursor: default; }
  .badge-btn:hover { background: var(--surface); }
  .badge-btn:focus-visible { outline: none; box-shadow: var(--anneau); }
  .badge-tip { position: absolute; z-index: var(--z-infobulle); bottom: calc(100% + 6px); left: 50%; transform: translateX(-50%); display: none; flex-direction: column; gap: 2px; width: max-content; max-width: min(240px, 70vw); padding: 8px 12px; border-radius: var(--r-s); background: var(--encre); color: var(--papier); box-shadow: var(--o2); font-size: 12px; line-height: 1.3; text-align: left; white-space: normal; pointer-events: none; }
  .badge-btn:hover .badge-tip, .badge-btn:focus .badge-tip { display: flex; }
  @media (pointer: coarse) { .badge-btn { width: 44px; height: 44px; } }
</style>
