<script lang="ts">
  import { initials, STATUS_LABEL } from '../lib/format'
  import { avatars } from '../blocks/avatars/avatars.svelte'

  let { pseudo, userId = '', serverId = '', size = '', speaking = false, status = '' }: { pseudo: string; userId?: string; serverId?: string; size?: 'sm' | 'lg' | ''; speaking?: boolean; status?: keyof typeof STATUS_LABEL | '' } = $props()

  $effect(() => {
    if (userId) avatars.ensure(userId, serverId || undefined)
  })
  const src = $derived(userId ? avatars.src(userId, serverId || undefined) : '')
</script>

{#snippet face()}
  <span class="plbls-avatar av {size ? `plbls-avatar--${size}` : ''}" class:speaking aria-hidden="true">
    {#if src}<img class="av-img" {src} alt="" />{:else}{initials(pseudo)}{/if}
  </span>
{/snippet}

{#if status}
  <span class="av-wrap">
    {@render face()}
    <span class="dot {status} av-dot av-dot--{size || 'md'}" role="img" aria-label={STATUS_LABEL[status]} title={STATUS_LABEL[status]}></span>
  </span>
{:else}
  {@render face()}
{/if}
