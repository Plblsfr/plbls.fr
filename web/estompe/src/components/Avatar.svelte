<script lang="ts">
  import { initials, STATUS_LABEL } from '../lib/format'
  import { avatars } from '../blocks/avatars/avatars.svelte'

  let { pseudo, userId = '', serverId = '', size = '', speaking = false, status = '' }: { pseudo: string; userId?: string; serverId?: string; size?: 'sm' | 'lg' | ''; speaking?: boolean; status?: keyof typeof STATUS_LABEL | '' } = $props()

  $effect(() => {
    if (userId) avatars.ensure(userId, serverId || undefined)
  })
  const src = $derived(userId ? avatars.src(userId, serverId || undefined) : '')
  const teinte = $derived.by(() => {
    let h = 0
    for (const c of userId || pseudo) h = (h * 31 + c.charCodeAt(0)) | 0
    return (Math.abs(h) % 6) + 1
  })
</script>

{#snippet face()}
  <span class="plbls-avatar av av-c{teinte} {size ? `plbls-avatar--${size}` : ''}" class:speaking aria-hidden="true">
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
