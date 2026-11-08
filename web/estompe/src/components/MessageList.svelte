<script lang="ts">
  import { tick, onDestroy, onMount, untrack } from 'svelte'
  import { app } from '../lib/store.svelte'
  import { continues, dayKey, dayOf } from '../lib/format'
  import { notify } from '../lib/ui'
  import { parseHash, router } from '../lib/route.svelte'
  import { windowOf } from '../lib/virtual'
  import type { Message } from '../lib/types'
  import MessageItem from './MessageItem.svelte'
  import Icon from './Icon.svelte'

  let { convKey }: { convKey: string } = $props()

  const conv = $derived(app.conv(convKey))
  const pseudos = $derived([...app.pseudosOf(convKey), ...app.groupMentionsOf(convKey)])
  const readers = $derived(app.readers(convKey))

  let el: HTMLElement
  let rowsEl = $state<HTMLElement>()
  let stick = true

  // Au-delà de VIRTUAL_FROM lignes, seules celles proches de l'écran sont dans le DOM (mémoire et temps de rendu bornés).
  const VIRTUAL_FROM = 150
  const ESTIMATE = 72
  type Row = { key: string; day?: string; nouveau?: boolean; m?: Message; cont?: boolean }
  let newFrom = $state<string | null>(null)
  let fresh = $state<string | null>(null)
  let failed = $state(false)
  const rows = $derived.by<Row[]>(() => {
    const out: Row[] = []
    conv.messages.forEach((m, i) => {
      const prev = conv.messages[i - 1]
      if (!prev || dayKey(prev.created_at) !== dayKey(m.created_at)) out.push({ key: 'day-' + m.id, day: dayOf(m.created_at) })
      if (m.id === newFrom) out.push({ key: 'new-' + m.id, nouveau: true })
      out.push({ key: m.id, m, cont: m.id !== newFrom && continues(prev, m) })
    })
    return out
  })
  const virtual = $derived(rows.length > VIRTUAL_FROM)
  const heights = new Map<string, number>()
  let scrollTop = $state(0)
  let viewport = $state(800)
  let measured = $state(0) // change quand une hauteur est mesurée : recalcule la fenêtre
  const win = $derived.by(() => {
    void measured
    if (!virtual) return { start: 0, end: rows.length, top: 0, bottom: 0 }
    const offset = rowsEl?.offsetTop ?? 0
    return windowOf(rows.map((r) => r.key), (k) => heights.get(k) ?? ESTIMATE, scrollTop - offset, viewport)
  })
  const shown = $derived(rows.slice(win.start, win.end))

  let pending = false
  const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver((entries) => {
    for (const e of entries) heights.set((e.target as HTMLElement).dataset.row!, Math.round(e.borderBoxSize?.[0]?.blockSize ?? e.contentRect.height))
    if (pending) return
    pending = true
    requestAnimationFrame(() => {
      pending = false
      measured++
      if (stick && el) el.scrollTop = el.scrollHeight
    })
  })
  /** Mesure la hauteur réelle d'une ligne (uniquement en mode fenêtré). */
  function measure(node: HTMLElement) {
    ro?.observe(node)
    return { destroy: () => ro?.unobserve(node) }
  }
  onDestroy(() => ro?.disconnect())
  $effect(() => {
    // les hauteurs des messages supprimés ne servent plus
    const keys = new Set(rows.map((r) => r.key))
    for (const k of heights.keys()) if (!keys.has(k)) heights.delete(k)
  })

  function atBottom() {
    return el.scrollHeight - el.scrollTop - el.clientHeight < 80
  }

  async function older() {
    const before = el.scrollHeight
    failed = false
    try {
      await app.loadOlder(convKey)
    } catch (e) {
      failed = true
      notify.error(e, "Impossible de charger l'historique")
      return
    }
    await tick()
    el.scrollTop += el.scrollHeight - before // reste sur le même message
  }

  // Appuyer sur du texte déjà sélectionné lance un glisser-déposer natif (curseur interdit) au lieu d'une nouvelle sélection.
  const startSelection = (e: MouseEvent) => { if (e.button === 0 && !(e.target as Element).closest('a, img, video')) getSelection()?.removeAllRanges() }

  onMount(() => { viewport = el.clientHeight })

  function onscroll() {
    scrollTop = el.scrollTop
    viewport = el.clientHeight
    stick = atBottom()
    if (el.scrollTop < 160 && conv.hasMore && !conv.loading && !failed) void older()
  }

  // Images, GIF et aperçus changent de hauteur après le rendu : on reste en bas tant que l'utilisateur n'a pas remonté.
  $effect(() => {
    if (!rowsEl || typeof ResizeObserver === 'undefined') return
    const o = new ResizeObserver(() => { if (stick) el.scrollTop = el.scrollHeight })
    o.observe(rowsEl)
    return () => o.disconnect()
  })

  // Nouveau message ou première page : on suit le bas tant que l'utilisateur n'a pas remonté.
  let seen = 0
  let lastId: string | undefined
  let freshTimer: ReturnType<typeof setTimeout> | undefined
  $effect(() => {
    const n = conv.messages.length
    const last = conv.messages.at(-1)
    if (seen > 0 && last && lastId !== undefined && last.id !== lastId) {
      if (last.author.id === app.meId) newFrom = null
      else if (!untrack(() => newFrom) && (!stick || !document.hasFocus())) newFrom = last.id
      fresh = last.id
      clearTimeout(freshTimer)
      freshTimer = setTimeout(() => (fresh = null), 600)
    }
    lastId = last?.id
    if (stick || seen === 0) tick().then(() => (el.scrollTop = el.scrollHeight))
    seen = n
  })
  onDestroy(() => clearTimeout(freshTimer))

  /** Adresse d'une notification ou d'un résultat de recherche (« …/<message> ») : charge l'historique jusqu'au message, s'y place et le met en évidence. */
  async function goTo(id: string, base: string) {
    history.replaceState(null, '', base) // un rechargement ou la dernière vue restaurée ne doivent pas y revenir
    router.current = parseHash(base)
    stick = false
    if (!conv.messages.some((m) => m.id === id)) {
      try { await app.loadFrom(convKey, id) } catch (e) { return notify.error(e, "Impossible de charger l'historique") }
    }
    await tick()
    const at = rows.findIndex((r) => r.m?.id === id)
    if (at < 0) return notify.info("Ce message n'existe plus ou n'est plus accessible.")
    if (virtual) { // message hors de la fenêtre : on s'en approche d'après les hauteurs connues pour qu'il soit rendu
      el.scrollTop = (rowsEl?.offsetTop ?? 0) + rows.slice(0, at).reduce((h, r) => h + (heights.get(r.key) ?? ESTIMATE), 0) - el.clientHeight / 2
      await tick()
      await new Promise(requestAnimationFrame)
    }
    const t = el.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`)
    if (!t) return
    t.scrollIntoView({ block: 'center' })
    t.classList.add('flash')
    setTimeout(() => t.classList.remove('flash'), 1500)
  }
  $effect(() => {
    const r = router.current
    if (!conv.loaded || (r.name !== 'channel' && r.name !== 'dm') || !r.messageId) return
    void goTo(r.messageId, location.hash.split('/').slice(0, r.name === 'dm' ? 3 : 4).join('/'))
  })
</script>

{#snippet item(r: Row)}
  {#if r.m}<MessageItem message={r.m} cont={r.cont!} {pseudos} {convKey} readers={readers[r.m.id]} fresh={r.m.id === fresh} />
  {:else if r.nouveau}<div class="newline" role="separator" aria-label="Nouveaux messages"><span>Nouveaux messages</span></div>
  {:else}<div class="day" role="separator" aria-label={r.day}><span>{r.day}</span></div>{/if}
{/snippet}

<div class="messages" bind:this={el} {onscroll} onmousedowncapture={startSelection} role="log" aria-live="polite" aria-label="Messages">
  {#if conv.loading && conv.messages.length === 0}
    <div class="skel-list" role="status" aria-label="Chargement des messages">
      {#each [60, 85, 40, 70] as w}<div class="skel-msg"><span class="skel-av v-sk"></span><span class="skel-lines"><span class="skel-line v-sk" style:width="{Math.round(w / 3)}%"></span><span class="skel-line v-sk" style:width="{w}%"></span></span></div>{/each}
    </div>
  {:else}
    {#if failed}
      <div class="v-alert dgr thread-alert" role="alert"><Icon name="alerte" size={18} /><span class="grow">Impossible de charger l'historique.</span><button class="v-btn sm sec" onclick={older} disabled={conv.loading}>Réessayer</button></div>
    {:else if conv.hasMore}
      <div class="thread-more"><button class="v-btn sm ghost" onclick={older} disabled={conv.loading}>Charger les messages précédents</button></div>
    {:else if conv.loaded}
      <div class="thread-start"><span class="thread-start-icon" aria-hidden="true"><Icon name="bulle" size={22} /></span><p class="thread-start-title">Début de la conversation</p></div>
    {/if}
    <div bind:this={rowsEl} style:padding-top="{win.top}px" style:padding-bottom="{win.bottom}px">
      {#each shown as r (r.key)}
        {#if virtual}
          <div data-row={r.key} use:measure>{@render item(r)}</div>
        {:else}
          {@render item(r)}
        {/if}
      {/each}
    </div>
  {/if}
</div>
