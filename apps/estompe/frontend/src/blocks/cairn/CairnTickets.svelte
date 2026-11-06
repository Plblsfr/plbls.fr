<script lang="ts">
  import Loader from '../../components/Loader.svelte'
  import { cairnApi } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { cairn } from './cairn.svelte'
  import { PRIORITIES, PRIORITY_TONE, STATUSES, STATUS_TONE, TYPES, cairnError, label, type Ticket, type TicketPriority, type TicketType } from './cairn'
  import { hrefs, router } from '../../lib/route.svelte'

  let { mine = false }: { mine?: boolean } = $props()
  let f = $state({ status: '', type: '', priority: '', assignee: '', label: '', blocked: '' })
  let tickets = $state<Ticket[]>([])
  let error = $state('')
  let loading = $state(true)
  let creating = $state(false)
  let draft = $state({ title: '', description: '', type: 'task' as TicketType, priority: 'normal' as TicketPriority })

  $effect(() => {
    const filters = { status: f.status, type: f.type, priority: f.priority, assignee: f.assignee, label: f.label.trim(), blocked: f.blocked }
    const space = cairn.space
    loading = true
    const req = mine ? cairnApi.myTickets() : cairnApi.tickets(space, filters)
    req.then((l) => { tickets = l; error = '' }).catch((e) => (error = cairnError(e))).finally(() => (loading = false))
  })

  async function create(e: SubmitEvent) {
    e.preventDefault()
    try {
      const t = await cairnApi.createTicket(cairn.space, { title: draft.title.trim(), description: draft.description.trim() || null, type: draft.type, priority: draft.priority })
      router.go(hrefs.cairn('ticket', t.key))
    } catch (err) { error = cairnError(err) }
  }

  // Un filtre actif se désactive en recliquant dessus.
  const pick = (k: 'status' | 'type' | 'priority', id: string) => (f[k] = f[k] === id ? '' : id)
</script>

{#snippet chips(list: { id: string; label: string }[], k: 'status' | 'type' | 'priority', name: string)}
  <div class="cx-chips" role="group" aria-label={name}>
    {#each list as s (s.id)}<button class="plbls-chip" aria-pressed={f[k] === s.id} onclick={() => pick(k, s.id)}>{s.label}</button>{/each}
  </div>
{/snippet}

<div class="cx-page">
  <div class="cx-wide">
    <div class="cx-bar">
      <h2 class="cx-h grow">{mine ? 'Mes tickets' : 'Tickets'}</h2>
      {#if !mine && cairn.can('ticket.write')}<button class="plbls-button plbls-button--compact" onclick={() => (creating = !creating)}>Nouveau ticket</button>{/if}
    </div>
    {#if !mine}
      <div class="cx-filters">
        {@render chips(STATUSES, 'status', 'Statut')}
        {@render chips(PRIORITIES, 'priority', 'Priorité')}
        {@render chips(TYPES, 'type', 'Type')}
        <div class="cx-chips">
          <button class="plbls-chip" aria-pressed={f.assignee === 'me'} onclick={() => (f.assignee = f.assignee === 'me' ? '' : 'me')}>Assignés à moi</button>
          <button class="plbls-chip" aria-pressed={f.blocked === 'true'} onclick={() => (f.blocked = f.blocked === 'true' ? '' : 'true')}>Bloqués</button>
          <input class="plbls-input cx-label-filter" placeholder="Étiquette" aria-label="Étiquette" bind:value={f.label} />
        </div>
      </div>
    {/if}
    {#if error}<p class="cx-err small" role="alert">{error}</p>{/if}
    {#if creating && !mine}
      <form class="cx-form" onsubmit={create}>
        <input class="plbls-input" placeholder="Titre" aria-label="Titre" bind:value={draft.title} required maxlength="200" />
        <textarea class="plbls-textarea" rows="4" placeholder="Description (Markdown)" aria-label="Description" bind:value={draft.description}></textarea>
        <div class="cx-bar">
          <select class="plbls-select" aria-label="Type" bind:value={draft.type}>{#each TYPES as s}<option value={s.id}>{s.label}</option>{/each}</select>
          <select class="plbls-select" aria-label="Priorité" bind:value={draft.priority}>{#each PRIORITIES as s}<option value={s.id}>{s.label}</option>{/each}</select>
          <button class="plbls-button plbls-button--compact">Créer dans {cairn.space}</button>
          <button type="button" class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => (creating = false)}>Annuler</button>
        </div>
      </form>
    {/if}
    {#if loading && !tickets.length}
      <Loader />
    {:else}
      <ul class="cx-rows">
        {#each tickets as t (t.key)}
          <li>
            <a class="cx-trow" href={hrefs.cairn('ticket', t.key)}>
              <span class="cx-key">{t.key}</span>
              <span class="cx-title truncate">{t.title}</span>
              {#each t.labels as l}<span class="cx-tag">{l}</span>{/each}
              {#if t.blocked}<span class="plbls-status plbls-status--negatif">Bloqué</span>{/if}
              {#if t.priority !== 'normal'}<span class="plbls-status {PRIORITY_TONE[t.priority]}">{label(PRIORITIES, t.priority)}</span>{/if}
              <span class="plbls-status {STATUS_TONE[t.status]}">{label(STATUSES, t.status)}</span>
              <span class="cx-who muted small truncate">{t.assignee ? cairn.who(t.assignee, app.meSub) : '—'}</span>
            </a>
          </li>
        {:else}
          <li class="plbls-empty-state"><h3>Aucun ticket</h3><p>{mine ? 'Aucun ticket ne vous est assigné.' : 'Aucun ticket ne correspond à ces filtres.'}</p></li>
        {/each}
      </ul>
    {/if}
    {#if tickets.length >= 1000}<p class="muted small">Seuls les 1000 tickets les plus récents sont affichés : affinez les filtres.</p>{/if}
  </div>
</div>
