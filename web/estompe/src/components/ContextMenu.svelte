<script lang="ts">
  import { ctxmenu, flipY, placeMenu } from '../lib/ctxmenu.svelte'

  let el = $state<HTMLElement>()
  let pos = $state({ left: 0, top: 0 })
  let opener: Element | null = null
  const close = (rendreFocus = false) => {
    if (rendreFocus === true && opener?.isConnected) (opener as HTMLElement).focus?.()
    ctxmenu.at = null
  }

  $effect(() => {
    const at = ctxmenu.at
    if (!at || !el) return
    opener = at.opener ?? document.activeElement
    pos = placeMenu(at.x, at.up ? at.y - el.offsetHeight : flipY(at.y, el.offsetHeight, window.innerHeight, at.haut), el.offsetWidth, el.offsetHeight, window.innerWidth, window.innerHeight)
    el.querySelector<HTMLElement>('button')?.focus()
  })

  // `scroll` ne remonte pas : on l'écoute en capture pour fermer au défilement de n'importe quel conteneur.
  $effect(() => {
    const onScroll = (e: Event) => { if (!el?.contains(e.target as Node)) close(true) }
    window.addEventListener('scroll', onScroll, true)
    return () => window.removeEventListener('scroll', onScroll, true)
  })

  function key(e: KeyboardEvent) {
    if (!ctxmenu.at) return
    if (e.key === 'Escape') {
      e.stopPropagation()
      return void close(true)
    }
    const b = [...el!.querySelectorAll<HTMLElement>('button')]
    const i = b.indexOf(document.activeElement as HTMLElement)
    const next = e.key === 'ArrowDown' ? (i + 1) % b.length : e.key === 'ArrowUp' ? (i - 1 + b.length) % b.length : -1
    if (next < 0) return
    e.preventDefault()
    b[next].focus()
  }
</script>

<svelte:window onkeydowncapture={key} onpointerdown={(e) => { if (ctxmenu.at && !el?.contains(e.target as Node)) close() }} onblur={() => close()} onresize={() => close()} onwheel={() => close()} />

{#if ctxmenu.at}
  <div class="popup ctxmenu" role="menu" bind:this={el} style="left: {pos.left}px; top: {pos.top}px">
    {#each ctxmenu.at.items as it (it.label)}
      <button role={it.checked === undefined ? 'menuitem' : 'menuitemradio'} class:danger={it.danger} class:sep={it.sep} aria-checked={it.checked} onclick={() => { close(true); it.run() }}>{#if it.checked}✓ {/if}{it.label}</button>
    {/each}
  </div>
{/if}
