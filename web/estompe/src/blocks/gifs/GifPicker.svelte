<script lang="ts">
  import { onMount } from 'svelte'
  import { api, ApiError } from '../../lib/api'
  import type { GifResult } from '../../lib/types'
  import Icon from '../../components/Icon.svelte'

  let { onpick, onclose }: { onpick: (g: GifResult) => void; onclose: () => void } = $props()

  const IDEAS = ['lol', 'ok', 'merci', 'bravo', 'triste', 'wtf', 'bonjour', 'fête']

  let q = $state('')
  let results = $state<GifResult[]>([])
  let busy = $state(true)
  let more = $state(true)
  let msg = $state('')
  let timer: ReturnType<typeof setTimeout>
  let seq = 0 // ignore les réponses d'une recherche déjà remplacée

  /** Lance une recherche (ou les tendances si le champ est vide) ; `next` ajoute la page suivante. */
  async function load(next = false) {
    const mine = ++seq
    const term = q.trim()
    busy = true
    if (!next) { results = []; more = true; msg = '' }
    try {
      const page = await api.gifSearch(term, next ? results.length : 0)
      if (mine !== seq) return
      const known = new Set(results.map((g) => g.id))
      results = [...results, ...page.filter((g) => !known.has(g.id))]
      more = page.length > 0
      msg = results.length ? '' : 'Aucun GIF trouvé.'
    } catch (e) {
      if (mine !== seq) return
      more = false
      msg = e instanceof ApiError && e.status === 501 ? "La recherche de GIF n'est pas activée sur ce serveur." : 'Recherche de GIF impossible.'
    } finally {
      if (mine === seq) busy = false
    }
  }

  function onInput() {
    clearTimeout(timer)
    timer = setTimeout(() => void load(), 350)
  }

  function idea(w: string) {
    q = w
    void load()
  }

  function scroll(e: Event) {
    const el = e.currentTarget as HTMLElement
    if (more && !busy && el.scrollTop + el.clientHeight > el.scrollHeight - 160) void load(true)
  }

  let box: HTMLElement
  let input: HTMLInputElement
  let shift = $state(0) // décalage horizontal pour rester dans la fenêtre (petit écran)

  onMount(() => {
    input.focus({ preventScroll: true }) // après le montage : l'attribut autofocus n'était pas fiable dans un popover
    const left = box.getBoundingClientRect().left
    if (left < 8) shift = 8 - left
  })

  void load()
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div bind:this={box} class="gifp" style:transform={shift ? `translateX(${shift}px)` : undefined} role="dialog" aria-label="Choisir un GIF" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && onclose()}>
  <div class="gifp-search">
    <Icon name="cherche" size={16} />
    <input type="search" placeholder="Rechercher un GIF" bind:this={input} bind:value={q} oninput={onInput} maxlength="100" aria-label="Rechercher un GIF" />
  </div>
  <div class="gifp-ideas" role="group" aria-label="Idées de recherche">
    <button class:on={!q} onclick={() => idea('')}>Tendances</button>
    {#each IDEAS as w}<button class:on={q === w} onclick={() => idea(w)}>{w}</button>{/each}
  </div>
  <div class="gifp-body" onscroll={scroll} aria-busy={busy}>
    <div class="gifp-grid">
      {#each results as g (g.id)}
        <button onclick={() => onpick(g)} title={g.title} style="aspect-ratio: {g.width || 1} / {g.height || 1}">
          <img src={g.preview} alt={g.title} loading="lazy" referrerpolicy="no-referrer" />
        </button>
      {/each}
      {#if busy}{#each Array(results.length ? 3 : 9) as _, i}<span class="gifp-skel" style="height: {70 + ((i * 37) % 60)}px"></span>{/each}{/if}
    </div>
    {#if msg}<p class="muted small gifp-msg">{msg}</p>{/if}
  </div>
  <p class="muted gifp-foot">Propulsé par GIPHY · les aperçus sont chargés depuis giphy.com</p>
</div>
