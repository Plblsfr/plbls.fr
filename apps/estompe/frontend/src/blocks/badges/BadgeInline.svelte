<script lang="ts">
  import BadgeIcon from './BadgeIcon.svelte'
  import { userBadges } from '../avatars/avatars.svelte'
  import { inlineBadges } from './badges'

  let { userId }: { userId: string } = $props()

  $effect(() => userBadges.ensure(userId))
  const inline = $derived(inlineBadges(userBadges.of(userId)))
</script>

{#each inline.shown as b (b.id)}
  <span class="badge-inline" role="img" aria-label={b.name} title="{b.name}{b.description ? ` — ${b.description}` : ''}"><BadgeIcon badge={b} size={16} /></span>
{/each}
{#if inline.extra}<span class="badge-more" title="{inline.extra} autre{inline.extra > 1 ? 's' : ''} badge{inline.extra > 1 ? 's' : ''}">+{inline.extra}</span>{/if}

<style>
  .badge-inline { flex: none; display: inline-flex; height: 16px; }
  .badge-more { flex: none; font-size: 10px; font-weight: 600; line-height: 16px; color: var(--gris); }
</style>
