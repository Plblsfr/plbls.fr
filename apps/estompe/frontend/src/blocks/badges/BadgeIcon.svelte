<script lang="ts">
  import { badgeIcons } from '../avatars/avatars.svelte'
  import type { Badge } from '../../lib/types'

  let { badge, size = 20 }: { badge: Pick<Badge, 'id' | 'icon' | 'icon_version'>; size?: number } = $props()

  let failed = $state(false)
  $effect(() => badgeIcons.ensure(badge.id, badge.icon_version))
  $effect(() => { badge.icon_version; failed = false })
  const src = $derived(badgeIcons.src(badge.id, badge.icon_version))
</script>

{#if badge.icon_version && !failed}
  {#if src}<img class="badge-icon" {src} alt="" width={size} height={size} onerror={() => (failed = true)} />{:else}<span class="badge-icon" style="width: {size}px; height: {size}px"></span>{/if}
{:else}<span class="badge-emoji" aria-hidden="true" style="font-size: {Math.round(size * 0.85)}px; line-height: {size}px">{badge.icon}</span>{/if}

<style>
  .badge-icon { display: inline-block; vertical-align: middle; object-fit: contain; }
  .badge-emoji { display: inline-block; vertical-align: middle; }
</style>
