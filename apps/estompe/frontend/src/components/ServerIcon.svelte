<script lang="ts">
  import { initials } from '../lib/format'
  import { serverIcons } from '../blocks/avatars/avatars.svelte'
  import type { Server } from '../lib/types'

  let { server, size = '' }: { server: Pick<Server, 'id' | 'name' | 'icon_version'>; size?: 'lg' | '' } = $props()

  $effect(() => serverIcons.ensure(server.id, server.icon_version))
  const src = $derived(serverIcons.src(server.id, server.icon_version))
</script>

<span class="plbls-avatar sv-icon" class:sv-icon--lg={size === 'lg'} aria-hidden="true">
  {#if src}<img class="av-img" {src} alt="" />{:else}{initials(server.name)}{/if}
</span>
