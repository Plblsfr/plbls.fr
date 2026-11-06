<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Modal from '../../components/Modal.svelte'
  import CairnTickets from './CairnTickets.svelte'
  import CairnTicket from './CairnTicket.svelte'
  import CairnDocs from './CairnDocs.svelte'
  import CairnTrash from './CairnTrash.svelte'
  import CairnSearch from './CairnSearch.svelte'
  import { cairn } from './cairn.svelte'
  import { cairnApi } from '../../lib/api'
  import { cairnError, pageTrail } from './cairn'
  import { cairnSections } from './cairn-nav'
  import { hrefs, router } from '../../lib/route.svelte'

  let { section, id, onmenu }: { section?: string; id?: string; onmenu: () => void } = $props()
  const allowed = $derived(cairnSections(cairn.can))
  const current = $derived(section ?? allowed[0]?.id)
  const title = $derived(allowed.find((s) => s.active(current))?.label ?? '')
  const trail = $derived(current === 'docs' && id ? pageTrail(cairn.pages, id) : [])
  let newTitle = $state('')
  let error = $state('')

  // Ctrl+K : la recherche, comme dans le reste de l'application.
  function key(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k' && allowed.some((s) => s.id === 'search')) { e.preventDefault(); router.go(hrefs.cairn('search')) }
  }

  async function create(e: SubmitEvent) {
    e.preventDefault()
    if (!cairn.creating) return
    try {
      const p = await cairnApi.createPage(cairn.space, newTitle.trim(), cairn.creating.parent, '')
      cairn.creating = null; newTitle = ''; error = ''
      await cairn.loadPages()
      router.go(hrefs.cairn('docs', p.id))
    } catch (err) { error = cairnError(err) }
  }
</script>

<svelte:window onkeydown={key} />

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1 class="cx-crumbs">
    <span class="muted">{cairn.space || 'Cairn'}</span>
    {#if trail.length}
      {#each trail as p, i (p.id)}<span class="muted" aria-hidden="true">›</span>{#if i < trail.length - 1}<a class="muted" href={hrefs.cairn('docs', p.id)}>{p.title}</a>{:else}<span class="truncate">{p.title}</span>{/if}{/each}
    {:else}
      <span class="muted" aria-hidden="true">›</span><span class="truncate">{title}</span>
    {/if}
  </h1>
</header>
{#if !cairn.spaces.length}
  <div class="cx-page"><div class="plbls-empty-state"><h3>Aucun espace Cairn</h3><p>Aucun espace n'est accessible avec votre compte.</p></div></div>
{:else if !allowed.some((s) => s.active(current))}
  <div class="cx-page"><div class="plbls-empty-state"><h3>Section indisponible</h3><p>Cette section n'est pas disponible avec vos droits.</p></div></div>
{:else if current === 'mine'}
  {#key cairn.space}<CairnTickets mine />{/key}
{:else if current === 'tickets'}
  {#key cairn.space}<CairnTickets />{/key}
{:else if current === 'ticket' && id}
  {#key id}<CairnTicket ticketKey={id} />{/key}
{:else if current === 'docs'}
  {#key cairn.space}<CairnDocs pageId={id} />{/key}
{:else if current === 'trash'}
  {#key cairn.space}<CairnTrash />{/key}
{:else if current === 'search'}
  <CairnSearch />
{/if}

{#if cairn.creating}
  <Modal title={cairn.creating.parent ? 'Nouvelle sous-page' : 'Nouvelle page'} onclose={() => { cairn.creating = null; error = '' }}>
    <form class="stack" onsubmit={create}>
      {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
      <div class="plbls-field">
        <label class="plbls-field__label" for="cx-new-title">Titre</label>
        <input id="cx-new-title" class="plbls-input" bind:value={newTitle} maxlength="200" autocomplete="off" required />
      </div>
      <button class="plbls-button" disabled={!newTitle.trim()}>Créer la page</button>
    </form>
  </Modal>
{/if}
