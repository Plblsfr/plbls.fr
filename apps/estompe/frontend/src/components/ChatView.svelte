<script lang="ts">
  import Icon from './Icon.svelte'
  import { app } from '../lib/store.svelte'
  import { BOT_ID } from '../lib/format'
  import MessageList from './MessageList.svelte'
  import Composer from './Composer.svelte'
  import ConvPanel from './ConvPanel.svelte'
  import { openSearch } from '../lib/search.svelte'

  let { convKey, title, isDM, onmenu }: {
    convKey: string
    title: string
    isDM: boolean
    onmenu: () => void
  } = $props()

  // Compte système « Estompe » (notes de version) : lecture seule, le serveur refuse aussi les envois.
  const readonly = $derived(app.dms.find((d) => `d:${d.id}` === convKey)?.other.id === BOT_ID)
  const canSend = $derived(app.canConv(convKey, 'send_messages'))
  let panel = $state<'pins' | null>(null)
  const typers = $derived(app.typingNames(convKey))
  const typingText = $derived(
    typers.length === 0 ? '' : typers.length === 1 ? `${typers[0]} est en train d'écrire…` : typers.length === 2 ? `${typers[0]} et ${typers[1]} écrivent…` : 'Plusieurs personnes écrivent…',
  )
</script>

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1 class="grow truncate">{isDM ? '@' : '#'} {title}</h1>
  <button class="ib" onclick={() => (panel = panel === 'pins' ? null : 'pins')} aria-label="Messages épinglés" aria-pressed={panel === 'pins'} title="Messages épinglés"><Icon name="pin" /></button>
  <button class="ib" onclick={openSearch} aria-label="Rechercher" title="Rechercher (Ctrl+K)"><Icon name="search" /></button>
</header>
<MessageList {convKey} />
<div class="typing" aria-live="polite">{typingText}</div>
{#if readonly}
  <!-- lecture seule -->
{:else if canSend}
  <Composer {convKey} placeholder={isDM ? `Écrire à ${title}` : `Écrire dans #${title}`} />
{:else}
  <p class="muted small" style="padding: 0.75rem 1rem">Vous n'avez pas la permission d'écrire dans ce salon.</p>
{/if}
{#if panel}
  {#key panel}<ConvPanel {convKey} onclose={() => (panel = null)} />{/key}
{/if}
