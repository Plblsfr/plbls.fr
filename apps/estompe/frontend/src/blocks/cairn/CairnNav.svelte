<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { cairn } from './cairn.svelte'
  import { buildTree, pageTrail, type PageNode } from './cairn'
  import { cairnSections } from './cairn-nav'
  import { router, hrefs } from '../../lib/route.svelte'

  const route = $derived(router.current)
  const sections = $derived(cairnSections(cairn.can))
  const tickets = $derived(sections.filter((s) => s.id === 'mine' || s.id === 'tickets'))
  const docs = $derived(sections.find((s) => s.id === 'docs'))
  const trash = $derived(sections.find((s) => s.id === 'trash'))
  const searchable = $derived(sections.some((s) => s.id === 'search'))
  const tree = $derived(buildTree(cairn.pages))
  const canWrite = $derived(cairn.can('page.write'))
  const current = $derived(route.name === 'cairn' ? route.id : undefined)

  $effect(() => { cairn.space; void cairn.loadPages() })
  // La page affichée reste visible : ses parents se déplient quand on y arrive.
  $effect(() => {
    if (route.name !== 'cairn' || route.section !== 'docs' || !current) return
    for (const p of pageTrail(cairn.pages, current).slice(0, -1)) if (cairn.closed[p.id]) cairn.closed[p.id] = false
  })
</script>

{#snippet branch(nodes: PageNode[], depth: number)}
  {#each nodes as n (n.id)}
    <div class="cx-row" style="--lvl: {depth}">
      {#if n.children.length}
        <button class="cx-fold" class:open={!cairn.closed[n.id]} onclick={() => (cairn.closed[n.id] = !cairn.closed[n.id])} aria-expanded={!cairn.closed[n.id]} aria-label="{cairn.closed[n.id] ? 'Déplier' : 'Replier'} {n.title}"><Icon name="chevron" size={14} /></button>
      {:else}<span class="cx-fold"></span>{/if}
      <a class="nav-item" class:active={route.name === 'cairn' && route.section === 'docs' && current === n.id} href={hrefs.cairn('docs', n.id)}>
        <Icon name="file" size={16} /><span class="truncate">{n.title}</span>
      </a>
      {#if canWrite && depth < 4}<button class="ib sm cx-add" onclick={() => (cairn.creating = { parent: n.id })} aria-label="Nouvelle sous-page de {n.title}" title="Nouvelle sous-page"><Icon name="plus" size={14} /></button>{/if}
    </div>
    {#if n.children.length && !cairn.closed[n.id]}{@render branch(n.children, depth + 1)}{/if}
  {/each}
{/snippet}

<div class="side-head">
  <select class="plbls-select grow" aria-label="Espace" bind:value={cairn.space}>
    {#each cairn.spaces as sp (sp.key)}<option value={sp.key}>{sp.name}{sp.archived ? ' (archivé)' : ''}</option>{/each}
  </select>
</div>
<div class="side-list">
  {#if searchable}
    <a class="cx-search" href={hrefs.cairn('search')}><Icon name="search" size={16} /><span class="grow">Rechercher</span><kbd>Ctrl K</kbd></a>
  {/if}
  {#if tickets.length}
    <div class="side-title">Tickets</div>
    {#each tickets as s (s.id)}
      <a class="nav-item" class:active={route.name === 'cairn' && s.active(route.section)} href={hrefs.cairn(s.id)}><Icon name={s.icon} size={16} /><span class="truncate">{s.label}</span></a>
    {/each}
  {/if}
  {#if docs}
    <div class="side-title cx-side-title">
      <span class="grow">Documentation</span>
      {#if canWrite}<button class="ib sm" onclick={() => (cairn.creating = { parent: null })} aria-label="Nouvelle page" title="Nouvelle page"><Icon name="plus" size={16} /></button>{/if}
    </div>
    {#if cairn.pagesError}<p class="cx-err small" role="alert">{cairn.pagesError}</p>{/if}
    {@render branch(tree, 0)}
    {#if !tree.length && !cairn.pagesError}<p class="muted small cx-empty">Aucune page</p>{/if}
    {#if trash}
      <a class="nav-item cx-trash" class:active={route.name === 'cairn' && trash.active(route.section)} href={hrefs.cairn('trash')}><Icon name="trash" size={16} /><span class="truncate">{trash.label}</span></a>
    {/if}
  {/if}
</div>
