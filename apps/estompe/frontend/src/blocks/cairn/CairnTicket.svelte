<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import Loader from '../../components/Loader.svelte'
  import Icon from '../../components/Icon.svelte'
  import Modal from '../../components/Modal.svelte'
  import { cairnApi, API_URL } from '../../lib/api'
  import { ctxmenu } from '../../lib/ctxmenu.svelte'
  import { app } from '../../lib/store.svelte'
  import { cairn } from './cairn.svelte'
  import { cairnImages } from './cairn-images'
  import { router, hrefs } from '../../lib/route.svelte'
  import {
    PRIORITIES, PRIORITY_TONE, STATUSES, STATUS_TONE, TYPES, cairnError, changedFields, conflictFields, label, renderDoc,
    type CairnComment, type CairnUser, type DepGraph, type LinkedPage, type Ticket, type TicketEvent, type TicketPatch,
  } from './cairn'

  let { ticketKey }: { ticketKey: string } = $props()
  let t = $state<Ticket | null>(null)
  let comments = $state<CairnComment[]>([])
  let events = $state<TicketEvent[]>([])
  let deps = $state<DepGraph>({ nodes: [], edges: [] })
  let pages = $state<LinkedPage[]>([])
  let error = $state('')
  let notice = $state('')
  let editing = $state(false)
  let draft = $state<TicketPatch>({})
  let labelsText = $state('')
  let newComment = $state('')
  let editingComment = $state<{ id: string; content: string } | null>(null)
  let depKey = $state('')
  let depDir = $state<'blocks' | 'blocked_by'>('blocked_by')
  let query = $state('')
  let found = $state<CairnUser[]>([])
  let confirming = $state(false)

  const when = (d: string) => new Date(d).toLocaleString('fr-FR')
  const who = (id: string | null) => cairn.who(id, app.meSub)
  const fail = (e: unknown) => (error = cairnError(e))
  const blockers = $derived(deps.edges.filter((e) => e.to === ticketKey).map((e) => ({ ...e, other: e.from, dir: 'blocked_by' as const })))
  const blocked = $derived(deps.edges.filter((e) => e.from === ticketKey).map((e) => ({ ...e, other: e.to, dir: 'blocks' as const })))
  const node = (k: string) => deps.nodes.find((n) => n.key === k)

  async function load() {
    try {
      t = await cairnApi.ticket(ticketKey)
      const [c, ev, d, p] = await Promise.allSettled([cairnApi.comments(ticketKey), cairnApi.events(ticketKey), cairnApi.dependencies(ticketKey), cairn.can('page.read') ? cairnApi.ticketPages(ticketKey) : Promise.resolve([])])
      comments = c.status === 'fulfilled' ? c.value : []
      events = ev.status === 'fulfilled' ? ev.value : []
      deps = d.status === 'fulfilled' ? d.value : { nodes: [], edges: [] }
      pages = p.status === 'fulfilled' ? p.value : []
      error = ''
    } catch (e) { fail(e) }
  }
  void load()

  function startEdit() {
    if (!t) return
    draft = { title: t.title, description: t.description ?? '', status: t.status, type: t.type, priority: t.priority, due: t.due ?? '' }
    labelsText = t.labels.join(', ')
    notice = ''
    editing = true
  }
  const normalized = (): TicketPatch => ({
    ...draft, description: draft.description || null, due: draft.due || null,
    labels: labelsText.split(',').map((l) => l.trim()).filter(Boolean),
  })

  /** Enregistre des champs ; sur 409, relit le ticket sans perdre les changements de l'utilisateur. */
  async function save(patch: TicketPatch) {
    if (!t || !Object.keys(patch).length) { editing = false; return }
    try {
      t = await cairnApi.updateTicket(ticketKey, t.version, patch)
      editing = false; notice = ''; error = ''
      events = await cairnApi.events(ticketKey).catch(() => events)
    } catch (e) {
      if ((e as { status?: number }).status !== 409) return fail(e)
      const base = t
      const fresh = await cairnApi.ticket(ticketKey).catch(() => null)
      if (!fresh) return fail(e)
      const clash = conflictFields(base, fresh, patch)
      t = fresh
      if (editing) {
        // les champs que je n'ai pas touchés suivent la version à jour, sinon l'enregistrement écraserait l'autre modification
        const next: Record<string, unknown> = { title: fresh.title, description: fresh.description ?? '', status: fresh.status, type: fresh.type, priority: fresh.priority, due: fresh.due ?? '' }
        for (const k of Object.keys(patch)) if (k in next) next[k] = (draft as Record<string, unknown>)[k]
        draft = next as TicketPatch
        if (!('labels' in patch)) labelsText = fresh.labels.join(', ')
        notice = `Ce ticket a été modifié entre-temps${clash.length ? ` (${clash.join(', ')})` : ''}. Vos changements sont conservés : enregistrez à nouveau pour les appliquer sur la dernière version.`
      } else notice = 'Ce ticket a été modifié entre-temps : la dernière version est affichée, refaites votre action.'
    }
  }
  const submit = (e: SubmitEvent) => { e.preventDefault(); void save(changedFields(t!, normalized())) }

  async function remove() {
    try { await cairnApi.deleteTicket(ticketKey); router.go(hrefs.cairn('tickets')) } catch (e) { confirming = false; fail(e) }
  }

  let timer: ReturnType<typeof setTimeout>
  function search() {
    clearTimeout(timer)
    found = []
    if (query.trim().length < 2) return
    timer = setTimeout(() => cairnApi.users(query.trim()).then((r) => { cairn.remember(r.users); found = r.users }).catch(fail), 250)
  }
  const assign = (id: string | null) => { found = []; query = ''; void save({ assignee: id }) }

  const act = async (f: () => Promise<unknown>) => { try { await f(); await load() } catch (e) { fail(e) } }
  const addComment = (e: SubmitEvent) => { e.preventDefault(); void act(async () => { await cairnApi.addComment(ticketKey, newComment.trim()); newComment = '' }) }
  const saveComment = () => editingComment && act(async () => { await cairnApi.editComment(ticketKey, editingComment!.id, editingComment!.content.trim()); editingComment = null })
  const dropComment = async (c: CairnComment) => (await confirmer({ titre: 'Supprimer le commentaire', message: 'Supprimer ce commentaire ?', action: 'Supprimer le commentaire', destructif: true })) && act(() => cairnApi.deleteComment(ticketKey, c.id))
  const addDep = (e: SubmitEvent) => { e.preventDefault(); void act(async () => { await cairnApi.addDependency(ticketKey, depDir, [depKey.trim().toUpperCase()]); depKey = '' }) }
  const dropDep = (other: string, dir: 'blocks' | 'blocked_by') => act(() => cairnApi.removeDependency(ticketKey, other, dir))
  const canWrite = $derived(cairn.can('ticket.write'))
  const menu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    ctxmenu.at = { x: r.right - 160, y: r.bottom + 4, items: [{ label: 'Supprimer', danger: true, run: () => (confirming = true) }] }
  }
</script>

<div class="cx-page">
  {#if error}<p class="cx-err small" role="alert">{error}</p>{/if}
  {#if notice}<p class="cx-warn small" role="status">{notice}</p>{/if}
  {#if t}
    <div class="cx-doc">
      <article class="cx-col">
        {#if editing}
          <form class="cx-edit" onsubmit={submit}>
            <input class="plbls-input cx-title-input" aria-label="Titre" bind:value={draft.title} required maxlength="200" />
            <textarea class="plbls-textarea cx-source" rows="10" aria-label="Description (Markdown)" bind:value={draft.description}></textarea>
            <div class="cx-bar">
              <select class="plbls-select" aria-label="Statut" bind:value={draft.status}>{#each STATUSES as s}<option value={s.id}>{s.label}</option>{/each}</select>
              <select class="plbls-select" aria-label="Type" bind:value={draft.type}>{#each TYPES as s}<option value={s.id}>{s.label}</option>{/each}</select>
              <select class="plbls-select" aria-label="Priorité" bind:value={draft.priority}>{#each PRIORITIES as s}<option value={s.id}>{s.label}</option>{/each}</select>
              <input class="plbls-input" type="date" aria-label="Échéance" bind:value={draft.due} />
              <input class="plbls-input" placeholder="Étiquettes, séparées par des virgules" aria-label="Étiquettes" bind:value={labelsText} />
            </div>
            <div class="cx-bar">
              <button class="plbls-button plbls-button--compact">Enregistrer</button>
              <button type="button" class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => { editing = false; notice = '' }}>Annuler</button>
            </div>
          </form>
        {:else}
          <header class="cx-pagehead">
            <div class="grow">
              <p class="cx-key muted">{t.key}</p>
              <h2 class="cx-title-h">{t.title}</h2>
              <p class="muted small">Créé par {who(t.created_by)} · {when(t.created_at)}</p>
            </div>
            {#if canWrite}<button class="plbls-button plbls-button--compact" onclick={startEdit}>Modifier</button>{/if}
            {#if cairn.can('ticket.delete')}<button class="ib" onclick={menu} aria-label="Plus d'actions" aria-haspopup="menu" title="Plus d'actions"><Icon name="more" /></button>{/if}
          </header>
          <div class="cx-md" use:cairnImages={t.description ?? ''}>{#if t.description}{@html renderDoc(t.description, API_URL)}{:else}<p class="muted">Pas de description.</p>{/if}</div>
        {/if}

        <section>
          <h3 class="cx-h">Dépendances</h3>
          <ul class="cx-cards">
            {#each [...blockers, ...blocked] as d (d.dir + d.other)}
              <li class="cx-card">
                <a href={hrefs.cairn('ticket', d.other)}>
                  <span class="muted small">{d.dir === 'blocked_by' ? 'Bloqué par' : 'Bloque'}</span>
                  <span class="cx-key">{d.other}</span><span class="cx-title">{node(d.other)?.title ?? ''}</span>
                  {#if node(d.other)}<span class="plbls-status {STATUS_TONE[node(d.other)!.status]}">{label(STATUSES, node(d.other)!.status)}</span>{/if}
                </a>
                {#if canWrite}<button class="ib sm" onclick={() => dropDep(d.other, d.dir)} aria-label="Retirer {d.other}" title="Retirer"><Icon name="x" size={14} /></button>{/if}
              </li>
            {:else}<li class="muted small">Aucune</li>{/each}
          </ul>
          {#if canWrite}
            <form class="cx-bar" onsubmit={addDep}>
              <select class="plbls-select" aria-label="Sens" bind:value={depDir}><option value="blocked_by">Est bloqué par</option><option value="blocks">Bloque</option></select>
              <input class="plbls-input" placeholder="Clé (ex. EST-12)" aria-label="Clé du ticket lié" bind:value={depKey} required pattern={'[A-Za-z]{2,6}-[0-9]+'} />
              <button class="plbls-button plbls-button--compact">Ajouter</button>
            </form>
          {/if}
        </section>

        {#if pages.length}
          <section>
            <h3 class="cx-h">Pages liées</h3>
            <ul class="cx-cards">{#each pages as p (p.id)}<li class="cx-card"><a href={hrefs.cairn('docs', p.id)}><Icon name="file" size={16} /><span class="cx-title">{p.title}</span><span class="muted small">{p.space}</span></a></li>{/each}</ul>
          </section>
        {/if}

        <section>
          <h3 class="cx-h">Commentaires</h3>
          <ul class="cx-list">
            {#each comments as c (c.id)}
              <li class="cx-form">
                <span class="muted small">{who(c.author)} · {when(c.created_at)}{c.updated_at !== c.created_at ? ' (modifié)' : ''}</span>
                {#if editingComment?.id === c.id}
                  <textarea class="plbls-textarea" rows="3" aria-label="Commentaire" bind:value={editingComment.content}></textarea>
                  <div class="cx-bar">
                    <button class="plbls-button plbls-button--compact" onclick={saveComment}>Enregistrer</button>
                    <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => (editingComment = null)}>Annuler</button>
                  </div>
                {:else}
                  <div class="cx-md" use:cairnImages={c.content}>{@html renderDoc(c.content, API_URL)}</div>
                  {#if cairn.can('comment.write') && c.author === app.meSub}
                    <div class="cx-bar">
                      <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => (editingComment = { id: c.id, content: c.content })}>Modifier</button>
                      <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => dropComment(c)}>Supprimer</button>
                    </div>
                  {/if}
                {/if}
              </li>
            {:else}<li class="muted small">Aucun commentaire</li>{/each}
          </ul>
          {#if cairn.can('comment.write')}
            <form class="cx-form" onsubmit={addComment}>
              <textarea class="plbls-textarea" rows="3" placeholder="Ajouter un commentaire (Markdown)" aria-label="Nouveau commentaire" bind:value={newComment} required></textarea>
              <button class="plbls-button plbls-button--compact">Commenter</button>
            </form>
          {/if}
        </section>

        <section>
          <h3 class="cx-h">Historique</h3>
          <ul class="cx-list small">
            {#each [...events].reverse() as ev}
              <li><span class="muted">{when(ev.created_at)}</span> {who(ev.author)}{ev.token ? ` (via ${ev.token})` : ''} ·
                {#if ev.kind === 'created'}création
                {:else if ev.kind === 'unblocked'}débloqué (fin de {ev.before})
                {:else if ev.kind === 'dependency'}dépendance {ev.field} {ev.after ? `+ ${ev.after}` : `− ${ev.before}`}
                {:else}{ev.field} : {ev.before ?? '∅'} → {ev.after ?? '∅'}{/if}
              </li>
            {/each}
          </ul>
        </section>
      </article>

      <aside class="cx-props" aria-label="Propriétés">
        <dl>
          <dt>Statut</dt>
          <dd>
            {#if canWrite}
              <select class="plbls-select" aria-label="Statut" value={t.status} onchange={(e) => void save({ status: e.currentTarget.value as Ticket['status'] })}>{#each STATUSES as s}<option value={s.id}>{s.label}</option>{/each}</select>
            {:else}<span class="plbls-status {STATUS_TONE[t.status]}">{label(STATUSES, t.status)}</span>{/if}
            {#if t.blocked}<span class="plbls-status plbls-status--negatif">Bloqué</span>{/if}
          </dd>
          <dt>Priorité</dt><dd><span class="plbls-status {PRIORITY_TONE[t.priority]}">{label(PRIORITIES, t.priority)}</span></dd>
          <dt>Type</dt><dd>{label(TYPES, t.type)}</dd>
          <dt>Échéance</dt><dd>{t.due ?? '—'}</dd>
          <dt>Étiquettes</dt><dd>{#each t.labels as l}<span class="cx-tag">{l}</span>{:else}—{/each}</dd>
          <dt>Assigné</dt>
          <dd>
            {t.assignee ? who(t.assignee) : 'Personne'}
            {#if canWrite}
              <div class="cx-bar">
                <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => assign(app.meSub)} disabled={t.assignee === app.meSub}>Me l'assigner</button>
                {#if t.assignee}<button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => assign(null)}>Retirer</button>{/if}
              </div>
              <input class="plbls-input" type="search" placeholder="Assigner : début de l'e-mail" aria-label="Rechercher un compte par e-mail" bind:value={query} oninput={search} />
              <ul class="cx-list">{#each found as u (u.id)}<li><button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => assign(u.id)}>{u.email}</button></li>{/each}</ul>
            {/if}
          </dd>
        </dl>
      </aside>
    </div>
  {:else if !error}<Loader />{/if}
</div>

{#if confirming}
  <Modal title="Supprimer ce ticket ?" onclose={() => (confirming = false)}>
    <div class="stack">
      <p>{ticketKey}, ses commentaires et son historique seront supprimés définitivement.</p>
      <div class="cx-bar">
        <button class="plbls-button cx-danger" onclick={remove}>Supprimer</button>
        <button class="plbls-button plbls-button--contour" onclick={() => (confirming = false)}>Annuler</button>
      </div>
    </div>
  </Modal>
{/if}
