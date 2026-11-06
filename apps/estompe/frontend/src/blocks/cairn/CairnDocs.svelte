<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import Loader from '../../components/Loader.svelte'
  import Icon from '../../components/Icon.svelte'
  import Modal from '../../components/Modal.svelte'
  import { cairnApi, API_URL } from '../../lib/api'
  import { cairn } from './cairn.svelte'
  import { cairnImages } from './cairn-images'
  import { ctxmenu } from '../../lib/ctxmenu.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { STATUSES, STATUS_TONE, cairnError, label, renderDoc, stripTitle, tocOf, type CairnPage, type LinkedTicket, type PageVersion, type SearchHit } from './cairn'

  let { pageId }: { pageId?: string } = $props()
  let page = $state<CairnPage | null>(null)
  let loading = $state(false)
  let error = $state('')
  let notice = $state('')
  let editing = $state(false)
  let draft = $state({ title: '', content: '' })
  let remote = $state<CairnPage | null>(null)
  let panel = $state<'' | 'versions' | 'move'>('')
  let versions = $state<PageVersion[]>([])
  let shown = $state<PageVersion | null>(null)
  let linked = $state<LinkedTicket[]>([])
  let confirming = $state(false)
  let linking = $state(false)
  let lq = $state('')
  let hits = $state<SearchHit[]>([])
  let move = $state({ parent: '', position: 0 })
  let fileInput = $state<HTMLInputElement>()

  const items = $derived(cairn.pages)
  const canWrite = $derived(cairn.can('page.write'))
  const canDelete = $derived(cairn.can('page.delete'))
  const html = $derived(page?.content ? renderDoc(stripTitle(page.content, page.title), API_URL) : '')
  const toc = $derived(html ? tocOf(html) : [])
  const when = (d: string) => new Date(d).toLocaleString('fr-FR')
  const fail = (e: unknown) => (error = cairnError(e))
  const descendants = (id: string): Set<string> => {
    const out = new Set([id])
    for (let grew = true; grew;) { grew = false; for (const p of items) if (p.parent_id && out.has(p.parent_id) && !out.has(p.id)) { out.add(p.id); grew = true } }
    return out
  }

  async function loadPage(id: string) {
    editing = false; notice = ''; panel = ''; remote = null; shown = null; loading = true
    try {
      page = await cairnApi.page(id)
      if (page.space !== cairn.space) cairn.space = page.space
      linked = cairn.can('ticket.read') ? await cairnApi.pageTickets(id).catch(() => []) : []
      error = ''
    } catch (e) { page = null; fail(e) } finally { loading = false }
  }
  $effect(() => { if (pageId) void loadPage(pageId); else { page = null; error = '' } })

  function startEdit() { if (page) { draft = { title: page.title, content: page.content }; remote = null; notice = ''; editing = true } }

  async function save(e: SubmitEvent) {
    e.preventDefault()
    if (!page) return
    try {
      page = await cairnApi.updatePage(page.id, page.version, { title: draft.title.trim(), content: draft.content })
      editing = false; notice = ''; remote = null
      await cairn.loadPages()
    } catch (err) {
      if ((err as { status?: number }).status !== 409) return fail(err)
      // le texte de l'utilisateur reste dans le formulaire ; la version à jour est proposée à la lecture
      remote = await cairnApi.page(page.id).catch(() => null)
      if (remote) page = { ...page, version: remote.version }
      notice = 'Cette page a été modifiée entre-temps. Votre texte est conservé : relisez la version à jour ci-dessous, puis enregistrez pour l\'écraser, ou copiez-en des passages.'
    }
  }

  async function upload() {
    const f = fileInput?.files?.[0]
    if (fileInput) fileInput.value = ''
    if (!f || !page) return
    try { draft.content += `\n![${f.name.replace(/[\[\]]/g, '')}](${await cairnApi.uploadImage(page.id, f)})\n` } catch (e) { fail(e) }
  }

  async function remove() {
    if (!page) return
    try { await cairnApi.deletePage(page.id); page = null; confirming = false; await cairn.loadPages(); router.go(hrefs.cairn('docs')) } catch (e) { confirming = false; fail(e) }
  }

  async function open(p: 'versions' | 'move') {
    panel = panel === p ? '' : p
    shown = null
    try {
      if (panel === 'versions' && page) versions = await cairnApi.versions(page.id)
      if (panel === 'move' && page) move = { parent: page.parent_id ?? '', position: 0 }
    } catch (e) { fail(e) }
  }
  const act = async (f: () => Promise<unknown>) => { try { await f(); error = '' } catch (e) { fail(e) } }
  const show = (n: number) => act(async () => (shown = await cairnApi.version(page!.id, n)))
  const restoreVersion = async (n: number) => (await confirmer({ titre: 'Restaurer la version', message: `Restaurer la version ${n} ? Une nouvelle version est créée.`, action: 'Restaurer' })) && act(async () => { page = await cairnApi.restoreVersion(page!.id, n); panel = ''; await cairn.loadPages() })
  const doMove = (e: SubmitEvent) => { e.preventDefault(); void act(async () => { await cairnApi.movePage(page!.id, move.parent || null, Number(move.position) || 0); panel = ''; await cairn.loadPages() }) }
  const unlink = (k: string) => act(async () => { await cairnApi.unlinkTicket(page!.id, k); linked = await cairnApi.pageTickets(page!.id) })
  const goto = (id: string) => document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })

  // « … » : les actions secondaires dans le menu d'Estompe ; celles qu'on n'a pas le droit de faire n'y figurent pas.
  function menu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const entries = [
      ...(canWrite ? [{ label: 'Sous-page', run: () => (cairn.creating = { parent: page!.id }) }, { label: 'Déplacer', run: () => void open('move') }] : []),
      ...(cairn.can('page.read') ? [{ label: 'Historique', run: () => void open('versions') }] : []),
      ...(canDelete ? [{ label: 'Supprimer', danger: true, run: () => (confirming = true) }] : []),
    ]
    ctxmenu.at = { x: r.right - 160, y: r.bottom + 4, items: entries }
  }

  let timer: ReturnType<typeof setTimeout>
  function searchTickets() {
    clearTimeout(timer)
    hits = []
    if (lq.trim().length < 2) return
    timer = setTimeout(() => cairnApi.search(lq.trim(), undefined, 'ticket', true).then((h) => (hits = h.filter((x) => !linked.some((k) => k.key === x.id)))).catch(fail), 250)
  }
  const link = (key: string) => act(async () => { await cairnApi.linkTicket(page!.id, key); linking = false; lq = ''; hits = []; linked = await cairnApi.pageTickets(page!.id) })
</script>

<div class="cx-page">
  {#if error}<p class="cx-err small" role="alert">{error}</p>{/if}
  {#if page}
    <div class="cx-doc">
      <article class="cx-col">
        {#if notice}<p class="cx-warn small" role="status">{notice}</p>{/if}
        {#if editing}
          <form class="cx-edit" onsubmit={save}>
            <input class="plbls-input cx-title-input" aria-label="Titre" bind:value={draft.title} required />
            <textarea class="plbls-textarea cx-source" rows="22" aria-label="Contenu (Markdown)" bind:value={draft.content}></textarea>
            <div class="cx-bar">
              <button class="plbls-button plbls-button--compact">Enregistrer</button>
              <button type="button" class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => fileInput?.click()}>Ajouter une image</button>
              <button type="button" class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => { editing = false; notice = '' }}>Annuler</button>
              <input bind:this={fileInput} type="file" accept="image/png,image/jpeg,image/gif,image/webp,image/svg+xml" hidden onchange={upload} />
            </div>
          </form>
          {#if remote}
            <h3 class="cx-h">Version à jour ({remote.version})</h3>
            <pre class="cx-md" style="white-space:pre-wrap">{remote.content}</pre>
          {/if}
        {:else}
          <header class="cx-pagehead">
            <div class="grow">
              <h2 class="cx-title-h">{page.title}</h2>
              <p class="muted small">Version {page.version} · {cairn.who(page.author)} · {when(page.updated_at)}</p>
            </div>
            {#if canWrite}<button class="plbls-button plbls-button--compact" onclick={startEdit}>Modifier</button>{/if}
            {#if canWrite || canDelete || cairn.can('page.read')}<button class="ib" onclick={menu} aria-label="Plus d'actions" aria-haspopup="menu" title="Plus d'actions"><Icon name="more" /></button>{/if}
          </header>
          {#if panel === 'move'}
            <form class="cx-form" onsubmit={doMove}>
              <select class="plbls-select" aria-label="Page parente" bind:value={move.parent}>
                <option value="">(racine)</option>
                {#each items.filter((p) => !descendants(page!.id).has(p.id)) as p (p.id)}<option value={p.id}>{p.title}</option>{/each}
              </select>
              <input class="plbls-input" type="number" min="0" aria-label="Position parmi les pages sœurs" bind:value={move.position} />
              <div class="cx-bar">
                <button class="plbls-button plbls-button--compact">Déplacer</button>
                <button type="button" class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => (panel = '')}>Annuler</button>
              </div>
            </form>
          {:else if panel === 'versions'}
            <ul class="cx-list">
              {#each versions as v (v.number)}
                <li class="cx-item small">
                  <span class="cx-title">Version {v.number} · {when(v.created_at)} · {v.title}</span>
                  <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => show(v.number)}>Voir</button>
                  {#if canDelete && v.number !== page.version}<button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => restoreVersion(v.number)}>Restaurer</button>{/if}
                </li>
              {/each}
            </ul>
            {#if shown}<div class="cx-form"><div class="cx-md" use:cairnImages={shown.content}>{@html renderDoc(shown.content ?? '', API_URL)}</div></div>{/if}
          {/if}
          <div class="cx-md" use:cairnImages={page.content}>{#if html}{@html html}{:else}<p class="muted">Cette page est vide.{#if canWrite} Cliquez sur « Modifier » pour la rédiger.{/if}</p>{/if}</div>
          {#if linked.length || canWrite}
            <section class="cx-linked">
              <div class="cx-bar">
                <h3 class="cx-h grow">Tickets liés</h3>
                {#if canWrite && cairn.can('ticket.read')}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (linking = true)}>Lier un ticket</button>{/if}
              </div>
              <ul class="cx-cards">
                {#each linked as k (k.key)}
                  <li class="cx-card">
                    <a href={hrefs.cairn('ticket', k.key)}><span class="cx-key">{k.key}</span><span class="cx-title">{k.title}</span><span class="plbls-status {STATUS_TONE[k.status]}">{label(STATUSES, k.status)}</span></a>
                    {#if canWrite && k.origin === 'manual'}<button class="ib sm" onclick={() => unlink(k.key)} aria-label="Retirer {k.key}" title="Retirer"><Icon name="x" size={14} /></button>{/if}
                  </li>
                {:else}<li class="muted small">Aucun ticket lié</li>{/each}
              </ul>
            </section>
          {/if}
        {/if}
      </article>
      {#if !editing && toc.length > 1}
        <nav class="cx-toc" aria-label="Sommaire">
          <div class="side-title">Sur cette page</div>
          {#each toc as h (h.id)}<button style="--lvl: {h.level - 2}" onclick={() => goto(h.id)}>{h.text}</button>{/each}
        </nav>
      {/if}
    </div>
  {:else if loading}
    <Loader />
  {:else if !error}
    <div class="plbls-empty-state">
      <h3>Documentation</h3>
      <p>Choisissez une page dans la barre latérale.</p>
      {#if canWrite}<button class="plbls-button plbls-button--compact" onclick={() => (cairn.creating = { parent: null })}>Nouvelle page</button>{/if}
    </div>
  {/if}
</div>

{#if confirming && page}
  <Modal title="Supprimer cette page ?" onclose={() => (confirming = false)}>
    <div class="stack">
      <p>« {page.title} » et ses sous-pages iront à la corbeille. Elles y restent 30 jours.</p>
      <div class="cx-bar">
        <button class="plbls-button cx-danger" onclick={remove}>Supprimer</button>
        <button class="plbls-button plbls-button--contour" onclick={() => (confirming = false)}>Annuler</button>
      </div>
    </div>
  </Modal>
{/if}

{#if linking}
  <Modal title="Lier un ticket" onclose={() => { linking = false; lq = ''; hits = [] }}>
    <div class="stack">
      <input class="plbls-input" type="search" placeholder="Clé ou titre du ticket" aria-label="Rechercher un ticket" bind:value={lq} oninput={searchTickets} autocomplete="off" />
      <ul class="cx-list">
        {#each hits as h (h.id)}
          <li><button class="cx-item cx-pick" onclick={() => link(h.id)}><span class="cx-key">{h.id}</span><span class="cx-title">{h.title}</span></button></li>
        {:else}{#if lq.trim().length >= 2}<li class="muted small">Aucun ticket trouvé</li>{/if}{/each}
      </ul>
    </div>
  </Modal>
{/if}
