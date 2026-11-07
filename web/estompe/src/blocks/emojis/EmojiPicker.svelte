<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import { EMOJI_GROUPS, loadEmojiRows, loadRecents, pushRecent, searchEmojis, type EmojiRow } from './emoji'
  import { emojis } from './emojis.svelte'

  let { onpick, onclose }: { onpick: (text: string, keep: boolean) => void; onclose: () => void } = $props()

  const COLS = 8
  const CELL = 36 // hauteur d'une ligne, pour réserver la place d'une catégorie pas encore affichée

  let rows = $state<EmojiRow[]>([])
  let recents = $state(loadRecents())
  let q = $state('')
  let hover = $state<{ char?: string; name: string; code: string } | null>(null)
  let active = $state(0)
  let shown = $state<Record<number, boolean>>({})
  let box: HTMLElement
  let input: HTMLInputElement
  let body: HTMLElement
  let shift = $state(0) // décalage horizontal pour rester dans la fenêtre (petit écran)

  const byChar = $derived(new Map(rows.map((r) => [r[0], r])))
  const idx = $derived(new Map(rows.map((r, i) => [r, i])))
  const sections = $derived.by(() => {
    const recent = recents.map((c) => byChar.get(c)).filter((r): r is EmojiRow => !!r)
    return [
      { label: 'Récents', icon: '🕘', items: recent },
      ...EMOJI_GROUPS.map((g, i) => ({ label: g.label, icon: g.icon, items: rows.filter((r) => r[5] === i) })),
    ].filter((s) => s.items.length)
  })
  const term = $derived(q.trim().replace(/^:|:$/g, '').toLowerCase())
  const found = $derived(term ? searchEmojis(rows, term) : [])
  const foundCustom = $derived(term ? emojis.list.filter((e) => e.name.includes(term)) : [])
  const custom = $derived(term ? [] : emojis.list)

  /** Affiche une catégorie quand elle approche de la zone visible (chargement par catégorie). */
  function lazy(node: HTMLElement, i: number) {
    if (typeof IntersectionObserver === 'undefined') { shown[i] = true; return }
    const io = new IntersectionObserver((e) => { if (e.some((x) => x.isIntersecting)) { shown[i] = true; io.disconnect() } }, { root: body, rootMargin: '240px' })
    io.observe(node)
    return { destroy: () => io.disconnect() }
  }

  function goto(i: number) {
    body.scrollTop = (body.querySelector<HTMLElement>(`[data-sec="${i}"]`)?.offsetTop ?? 0) - 4
  }

  function onscroll() {
    const secs = [...body.querySelectorAll<HTMLElement>('[data-sec]')]
    active = Math.max(0, secs.filter((s) => s.offsetTop <= body.scrollTop + 12).length - 1)
  }

  function show(e: Event) {
    const b = (e.target as Element).closest<HTMLElement>('[data-i]')
    if (!b) return
    const r = rows[+b.dataset.i!]
    hover = r ? { char: r[0], name: r[4], code: `:${r[1]}:` } : { name: b.dataset.n ?? '', code: b.dataset.c ?? '' }
  }

  function choose(b: HTMLElement, keep: boolean) {
    const r = rows[+b.dataset.i!]
    if (r) {
      recents = pushRecent(r[0])
      onpick(r[0], keep)
    } else if (b.dataset.c) onpick(b.dataset.c, keep)
  }

  function onclick(e: MouseEvent) {
    const b = (e.target as Element).closest<HTMLElement>('[data-i]')
    if (b) choose(b, e.shiftKey)
  }

  function buttons() {
    return [...body.querySelectorAll<HTMLButtonElement>('.ep-grid button')]
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onclose(); return }
    const t = e.target as HTMLElement
    if (t === input) {
      if (e.key === 'ArrowDown') { e.preventDefault(); buttons()[0]?.focus() }
      if (e.key === 'Enter') { e.preventDefault(); const b = buttons()[0]; if (b) choose(b, e.shiftKey) }
      return
    }
    if (!t.matches('.ep-grid button')) return
    const all = buttons()
    const i = all.indexOf(t as HTMLButtonElement)
    const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -COLS, ArrowDown: COLS }[e.key]
    if (step) {
      e.preventDefault()
      const grid = [...t.parentElement!.children]
      const j = grid.indexOf(t) + step
      const next = j >= 0 && j < grid.length ? grid[j] : Math.abs(step) === 1 ? all[i + step] : undefined
      if (next) (next as HTMLElement).focus()
      else if (step === -COLS) input.focus()
    } else if (e.key === 'Enter') { e.preventDefault(); choose(t, e.shiftKey) }
  }

  onMount(() => {
    input.focus({ preventScroll: true })
    const left = box.getBoundingClientRect().left
    if (left < 8) shift = 8 - left
    void loadEmojiRows().then((r) => { rows = r })
  })
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div bind:this={box} class="ep" style:transform={shift ? `translateX(${shift}px)` : undefined} role="dialog" aria-label="Choisir un smiley" tabindex="-1" {onkeydown}>
  <div class="gifp-search">
    <Icon name="cherche" size={16} />
    <input type="search" placeholder="Rechercher un smiley" bind:this={input} bind:value={q} maxlength="50" aria-label="Rechercher un smiley" />
  </div>
  {#if !term}
    <div class="ep-tabs" role="group" aria-label="Catégories">
      {#each sections as s, i}<button class:on={i === active} onclick={() => goto(i)} aria-label={s.label} title={s.label}>{s.icon}</button>{/each}
    </div>
  {/if}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_mouse_events_have_key_events, a11y_click_events_have_key_events -->
  <div class="ep-body" bind:this={body} {onscroll} {onclick} onmouseover={show} onfocusin={show}>
    {#if term}
      <div class="ep-grid">
        {#each found as r}<button tabindex="-1" data-i={idx.get(r)} aria-label={r[4]}>{r[0]}</button>{/each}
        {#each foundCustom as e (e.id)}
          {@const _ = emojis.ensure(e.id)}
          <button tabindex="-1" data-i="-1" data-n={e.name} data-c=":{e.name}:" aria-label=":{e.name}:">{#if emojis.src(e.id)}<img src={emojis.src(e.id)} alt="" />{:else}:{/if}</button>
        {/each}
      </div>
      {#if !found.length && !foundCustom.length}<p class="muted small gifp-msg">{rows.length ? 'Aucun smiley trouvé.' : 'Chargement…'}</p>{/if}
    {:else}
      {#each sections as s, i (s.label)}
        <section data-sec={i} use:lazy={i}>
          <h3>{s.label}</h3>
          <div class="ep-grid" style:min-height="{Math.ceil(s.items.length / COLS) * CELL}px">
            {#if shown[i]}{#each s.items as r}<button tabindex="-1" data-i={idx.get(r)} aria-label={r[4]}>{r[0]}</button>{/each}{/if}
          </div>
        </section>
      {:else}
        <p class="muted small gifp-msg">Chargement…</p>
      {/each}
      {#if custom.length}
        <section data-sec={sections.length}>
          <h3>Smileys des serveurs</h3>
          <div class="ep-grid">
            {#each custom as e (e.id)}
              {@const _ = emojis.ensure(e.id)}
              <button tabindex="-1" data-i="-1" data-n={e.name} data-c=":{e.name}:" aria-label=":{e.name}:">{#if emojis.src(e.id)}<img src={emojis.src(e.id)} alt="" />{:else}:{/if}</button>
            {/each}
          </div>
        </section>
      {/if}
    {/if}
  </div>
  <p class="ep-foot" aria-live="polite">
    {#if hover}<span class="ep-big">{hover.char ?? ''}</span><strong class="truncate">{hover.name}</strong><span class="muted mono">{hover.code}</span>{:else}<span class="muted">Maj + clic pour en insérer plusieurs</span>{/if}
  </p>
</div>
