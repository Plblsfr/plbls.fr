<script lang="ts">
  import Icon from './Icon.svelte'
  import { app } from '../lib/store.svelte'
  import { BOT_ID } from '../lib/format'
  import MessageList from './MessageList.svelte'
  import Composer from './Composer.svelte'
  import { openSearch } from '../lib/search.svelte'
  import { marge } from '../lib/marge.svelte'

  let { convKey, title, isDM, sujet = '', margeDispo = false, onmenu }: {
    convKey: string
    title: string
    isDM: boolean
    sujet?: string
    margeDispo?: boolean
    onmenu: () => void
  } = $props()

  // Compte système « Estompe » (notes de version) : lecture seule, le serveur refuse aussi les envois.
  const readonly = $derived(app.dms.find((d) => `d:${d.id}` === convKey)?.other.id === BOT_ID)
  const canSend = $derived(app.canConv(convKey, 'send_messages'))
  const epingles = $derived(marge.visible && marge.onglet === 'epingles')
  const typers = $derived(app.typingNames(convKey))
  const typingText = $derived(
    typers.length === 0 ? '' : typers.length === 1 ? `${typers[0]} est en train d'écrire…` : typers.length === 2 ? `${typers[0]} et ${typers[1]} écrivent…` : 'Plusieurs personnes écrivent…',
  )
</script>

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <div class="main-head-t">
    <h1 class="truncate">{isDM ? '@' : '#'} {title}</h1>
    {#if sujet}<span class="main-head-sujet truncate" title={sujet}>{sujet}</span>{/if}
  </div>
  <div class="main-head-actions">
    <button class="ib" onclick={() => marge.basculerEpingles()} aria-label="Messages épinglés" aria-pressed={epingles} title="Messages épinglés"><Icon name="pin" /></button>
    {#if margeDispo}<button class="ib" class:on={marge.visible} onclick={() => marge.basculer()} aria-label={marge.visible ? 'Masquer la marge' : 'Afficher la marge'} title="{marge.visible ? 'Masquer la marge' : 'Afficher la marge'} (Ctrl+.)"><Icon name="panneau" /></button>{/if}
    <button class="main-search" onclick={openSearch} title="Rechercher (Ctrl+K)"><Icon name="search" size={16} /><span class="main-search-l">Rechercher ou aller à…</span><kbd class="v-kbd">Ctrl K</kbd></button>
  </div>
</header>
<MessageList {convKey} />
<div class="typing" aria-live="polite">{typingText}</div>
{#if readonly}
  <!-- lecture seule -->
{:else if canSend}
  <Composer {convKey} placeholder={isDM ? `Écrire à ${title}` : `Écrire dans #${title}`} />
{:else}
  <p class="muted small" style="padding: 0.75rem 1rem">Tu n'as pas la permission d'écrire dans ce salon.</p>
{/if}
