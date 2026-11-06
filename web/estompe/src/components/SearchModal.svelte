<script lang="ts">
  import Loader from './Loader.svelte'
  import { untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import Modal from './Modal.svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { hrefs, router } from '../lib/route.svelte'
  import { searchNav } from '../lib/search.svelte'
  import { clearRecentSearches, excerpt, highlightTerms, recentSearches, rememberSearch, suggestions, type Scope } from '../lib/search'
  import { dayOf, timeOf } from '../lib/format'
  import { notify } from '../lib/ui'
  import type { Message } from '../lib/types'

  let { onclose }: { onclose: () => void } = $props()

  const { convKey, serverId } = untrack(() => ({ convKey: searchNav.convKey, serverId: searchNav.serverId }))
  const inDM = convKey.startsWith('d:')
  const channelName = convKey.startsWith('c:') ? app.channel(convKey.slice(2))?.channel.name : undefined
  const dmName = inDM ? app.dms.find((d) => d.id === convKey.slice(2))?.other.pseudo : undefined
  const serverName = serverId ? app.servers.find((s) => s.id === serverId)?.name : undefined

  const scopes = $derived<[Scope, string][]>([
    ['all', 'Partout'],
    ...(serverName ? [['server', `Ce serveur (${serverName})`] as [Scope, string]] : []),
    ...(convKey ? [['conv', inDM ? `Cette conversation (${dmName ?? ''})` : `Ce salon (#${channelName ?? ''})`] as [Scope, string]] : []),
  ])

  let query = $state('')
  let scope = $state<Scope>(convKey ? 'conv' : 'all')
  let sort = $state<'relevance' | 'date'>('relevance')
  let results = $state<Message[]>([])
  let more = $state(false)
  let busy = $state(false)
  let searched = $state('')
  let recents = $state(recentSearches())
  let active = $state(-1)
  let sentinel = $state<HTMLElement>()
  let seq = 0

  const scopeParams = $derived(
    scope === 'server' ? { server_id: serverId } : scope === 'conv' ? (inDM ? { dm_id: convKey.slice(2) } : { channel_id: convKey.slice(2) }) : {},
  )
  const hints = $derived(suggestions(query, {
    pseudos: [...new Set(scope === 'all' ? app.servers.flatMap((s) => app.details[s.id]?.members.map((m) => m.pseudo) ?? []).concat(app.dms.map((d) => d.other.pseudo)) : app.pseudosOf(convKey))],
    channels: [...new Set((scope === 'all' ? app.servers : app.servers.filter((s) => s.id === serverId)).flatMap((s) => app.details[s.id]?.channels.filter((c) => c.kind === 'text').map((c) => c.name) ?? []))],
    servers: app.servers.map((s) => s.name),
  }))
  const terms = $derived(highlightTerms(searched))

  async function run(older = false) {
    const q = query.trim()
    if (!q || busy && older) return
    const mine = ++seq
    busy = true
    try {
      const page = await api.search(q, scopeParams, sort, older ? results.length : 0)
      if (mine !== seq) return
      results = older ? [...results, ...page.messages] : page.messages
      more = page.has_more
      searched = q
      if (!older) recents = rememberSearch(q)
    } catch (e) {
      if (mine === seq) notify.error(e, 'Recherche impossible')
    } finally {
      if (mine === seq) busy = false
    }
  }

  // Changer de portée ou de tri relance la recherche en cours.
  let first = true
  $effect(() => {
    void scope, sort
    if (first) { first = false; return }
    if (searched) untrack(() => void run())
  })

  // Pagination au défilement : la sentinelle de fin de liste charge la page suivante.
  $effect(() => {
    if (!sentinel || !more) return
    const o = new IntersectionObserver((e) => { if (e[0].isIntersecting && !busy) void run(true) }, { rootMargin: '200px' })
    o.observe(sentinel)
    return () => o.disconnect()
  })

  function apply(s: { insert: string }) {
    query = s.insert
    active = -1
    document.getElementById('search-input')?.focus()
  }

  function onkeydown(e: KeyboardEvent) {
    if (hints.length && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault()
      active = (active + (e.key === 'ArrowDown' ? 1 : -1) + hints.length) % hints.length
    } else if (hints.length && active >= 0 && (e.key === 'Enter' || e.key === 'Tab')) {
      e.preventDefault()
      apply(hints[active])
    } else if (e.key === 'Enter') {
      e.preventDefault()
      void run()
    }
  }

  /** Salon (avec son serveur) ou conversation privée d'un message : titre de groupe et adresse. */
  function where(m: Message): { group: string; href: string } {
    if (m.dm_id) return { group: `@ ${app.dms.find((d) => d.id === m.dm_id)?.other.pseudo ?? 'Message privé'}`, href: hrefs.dm(m.dm_id) }
    const c = app.channel(m.channel_id!)
    return { group: c ? `${c.server.name} › # ${c.channel.name}` : 'Salon', href: c ? hrefs.channel(c.server.id, c.channel.id) : '' }
  }

  const groups = $derived.by(() => {
    const out: { group: string; items: { m: Message; href: string }[] }[] = []
    for (const m of results) {
      const w = where(m)
      let g = out.find((x) => x.group === w.group)
      if (!g) out.push((g = { group: w.group, items: [] }))
      g.items.push({ m, href: w.href })
    }
    return out
  })

  function open(m: Message, href: string) {
    if (!href) return
    onclose()
    router.go(`${href}/${m.id}`)
  }
</script>

<Modal title="Rechercher" wide {onclose}>
  <div class="search">
    <form class="search-bar" onsubmit={(e) => { e.preventDefault(); void run() }}>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="search-input" class="plbls-input grow" type="search" placeholder="Mots, « expression », -exclu, de:pseudo, dans:#salon, a:image…" bind:value={query} {onkeydown} oninput={() => (active = -1)} maxlength="200" autofocus autocomplete="off" aria-label="Rechercher" aria-controls="search-hints" />
      <button class="plbls-button plbls-button--compact" class:is-busy={busy} disabled={!query.trim()}>Chercher</button>
    </form>
    <div class="search-opts">
      <label>Où <select class="plbls-select" bind:value={scope}>{#each scopes as [v, label] (v)}<option value={v}>{label}</option>{/each}</select></label>
      <label>Tri <select class="plbls-select" bind:value={sort}><option value="relevance">Pertinence</option><option value="date">Plus récents</option></select></label>
    </div>
    {#if hints.length}
      <ul class="search-hints" id="search-hints" role="listbox" aria-label="Filtres">
        {#each hints as s, i (s.insert)}
          <li role="option" aria-selected={i === active}><button type="button" class:active={i === active} onclick={() => apply(s)}>{s.label}</button></li>
        {/each}
      </ul>
    {/if}
    <div class="search-body" role="region" aria-label="Résultats" aria-live="polite">
      {#each groups as g (g.group)}
        <h3 class="search-group">{g.group}</h3>
        {#each g.items as { m, href } (m.id)}
          <button class="result search-hit" onclick={() => open(m, href)}>
            <span class="small"><strong>{m.author.pseudo}</strong> · <span class="muted">{dayOf(m.created_at)} {timeOf(m.created_at)}</span></span>
            <span class="search-text">{#each excerpt(m.content, terms) as p}{#if p.hit}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}</span>
            {#if m.attachments.length}<span class="muted small row"><Icon name="clip" size={14} /> {m.attachments.length} fichier{m.attachments.length > 1 ? 's' : ''}</span>{/if}
          </button>
        {/each}
      {/each}
      {#if more}<div bind:this={sentinel} class="center"><Loader /></div>{/if}
      {#if !results.length && !busy}
        {#if searched}
          <p class="muted">Aucun résultat pour « {searched} ».</p>
        {:else if recents.length}
          <div class="row"><h3 class="search-group grow">Dernières recherches</h3><button class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => { clearRecentSearches(); recents = [] }}>Effacer</button></div>
          {#each recents as r (r)}<button class="plbls-button plbls-button--lien search-recent" onclick={() => { query = r; void run() }}><Icon name="search" size={14} /> {r}</button>{/each}
        {:else}
          <p class="muted">Filtres : de:pseudo · dans:#salon · serveur:nom · mentionne:pseudo · avant:/après:/pendant:AAAA-MM-JJ · a:fichier|image|vidéo|lien|sondage · épinglé:oui</p>
        {/if}
      {/if}
    </div>
  </div>
</Modal>
