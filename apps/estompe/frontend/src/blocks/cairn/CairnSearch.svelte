<script lang="ts">
  import { cairnApi } from '../../lib/api'
  import { cairn } from './cairn.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { cairnError, type SearchHit } from './cairn'

  let q = $state('')
  let type = $state('')
  let all = $state(true)
  let hits = $state<SearchHit[] | null>(null)
  let error = $state('')

  // Extrait : texte échappé, termes trouvés (entre « ») mis en évidence.
  const snippet = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/«/g, '<mark>').replace(/»/g, '</mark>')

  async function run(e: SubmitEvent) {
    e.preventDefault()
    try { hits = await cairnApi.search(q.trim(), all ? undefined : cairn.space, type || undefined); error = '' } catch (err) { error = cairnError(err) }
  }
</script>

<div class="cx-page"><div class="cx-wide">
  <form class="cx-bar" onsubmit={run}>
    <input class="plbls-input" type="search" placeholder="Rechercher dans les tickets et les pages" aria-label="Recherche" bind:value={q} required />
    <select class="plbls-select" aria-label="Type" bind:value={type}><option value="">Tickets et pages</option><option value="ticket">Tickets</option><option value="page">Pages</option></select>
    <select class="plbls-select" aria-label="Espace" value={all ? '' : 'one'} onchange={(e) => (all = e.currentTarget.value === '')}><option value="">Tous les espaces</option><option value="one">{cairn.space}</option></select>
    <button class="plbls-button plbls-button--compact">Rechercher</button>
  </form>
  {#if error}<p class="cx-err small" role="alert">{error}</p>{/if}
  {#if hits}
    <ul class="cx-list">
      {#each hits as h (h.type + h.id)}
        <li>
          <a class="cx-item" href={h.type === 'ticket' ? hrefs.cairn('ticket', h.id) : hrefs.cairn('docs', h.id)}>
            <span class="plbls-badge">{h.type === 'ticket' ? h.id : 'page'}</span>
            <span class="cx-title">{h.title} <span class="muted small">{h.space}</span></span>
            {#if h.snippet}<span class="cx-snip small muted" style="flex-basis:100%">{@html snippet(h.snippet)}</span>{/if}
          </a>
        </li>
      {:else}<li class="muted">Aucun résultat</li>{/each}
    </ul>
  {/if}
</div></div>
