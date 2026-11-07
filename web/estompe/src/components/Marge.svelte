<script lang="ts">
  import Icon from './Icon.svelte'
  import MembersPanel from './MembersPanel.svelte'
  import ConvPanel from './ConvPanel.svelte'
  import { marge, MARGE_MAX, MARGE_MIN, type OngletMarge } from '../lib/marge.svelte'
  import type { ServerDetail } from '../lib/types'

  let { detail, convKey, onglet }: { detail?: ServerDetail; convKey: string | null; onglet: OngletMarge } = $props()

  const onglets = $derived([
    ...(detail ? [{ id: 'membres' as const, label: 'Membres' }] : []),
    ...(convKey ? [{ id: 'epingles' as const, label: 'Épinglés' }] : []),
  ])

  let depart: { x: number; l: number } | null = null
  function saisir(e: PointerEvent) {
    depart = { x: e.clientX, l: marge.largeur }
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  }
  function tirer(e: PointerEvent) {
    if (depart) marge.largeur = Math.min(MARGE_MAX, Math.max(MARGE_MIN, depart.l + depart.x - e.clientX))
  }
  function lacher() {
    if (depart) marge.redimensionner(marge.largeur)
    depart = null
  }
  function clavier(e: KeyboardEvent) {
    const pas = e.shiftKey ? 40 : 16
    if (e.key === 'ArrowLeft') marge.redimensionner(marge.largeur + pas)
    else if (e.key === 'ArrowRight') marge.redimensionner(marge.largeur - pas)
    else if (e.key === 'Home') marge.redimensionner(MARGE_MIN)
    else if (e.key === 'End') marge.redimensionner(MARGE_MAX)
    else return
    e.preventDefault()
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && !marge.large && !e.defaultPrevented && !document.querySelector('plbls-modal, .confirm, .ctxmenu')) marge.fermer() }} />

{#if !marge.large}<button class="marge-voile" aria-label="Fermer la marge" tabindex="-1" onclick={() => marge.fermer()}></button>{/if}
<div class="marge" role="region" aria-label="Marge">
  {#if marge.large}
    <button class="marge-poignee" aria-label="Largeur de la marge : {marge.largeur} px (flèches gauche et droite)" title="Glisser pour élargir la marge"
      onpointerdown={saisir} onpointermove={tirer} onpointerup={lacher} onpointercancel={lacher} onkeydown={clavier}></button>
  {/if}
  <div class="marge-tete">
    <div class="v-seg grow" role="tablist" aria-label="Contenu de la marge">
      {#each onglets as o (o.id)}
        <button role="tab" id="marge-onglet-{o.id}" aria-selected={onglet === o.id} aria-controls="marge-corps" onclick={() => marge.montrer(o.id)}>{o.label}</button>
      {/each}
    </div>
    <button class="ib" onclick={() => marge.fermer()} aria-label="Fermer la marge" title="Fermer la marge (Ctrl+.)"><Icon name="x" size={18} /></button>
  </div>
  <div class="marge-corps" id="marge-corps" role="tabpanel" aria-labelledby="marge-onglet-{onglet}">
    {#if onglet === 'membres' && detail}
      <MembersPanel {detail} />
    {:else if onglet === 'epingles' && convKey}
      {#key convKey}<ConvPanel {convKey} onclose={() => marge.basculerEpingles()} />{/key}
    {/if}
  </div>
</div>

<style>
  .marge { position: relative; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--papier-2); box-shadow: inset 1px 0 0 var(--trait); }
  .marge-tete { display: flex; align-items: center; gap: var(--espace-1); min-height: 60px; padding: 0 var(--espace-2) 0 var(--espace-3); box-shadow: 0 1px 0 var(--trait); padding-top: env(safe-area-inset-top); }
  .marge-tete .v-seg > button { flex: 1; justify-content: center; }
  .marge-tete .v-seg > button:focus-visible { outline: none; box-shadow: var(--anneau); }
  .marge-corps { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; padding: var(--espace-2) var(--espace-2) calc(var(--espace-3) + env(safe-area-inset-bottom)); }
  .marge-poignee { position: absolute; padding: 0; border: 0; background: transparent; z-index: 2; left: -4px; top: 0; bottom: 0; width: 8px; cursor: col-resize; touch-action: none; }
  .marge-poignee::after { content: ''; position: absolute; left: 3px; top: 0; bottom: 0; width: 2px; background: transparent; transition: background var(--d) var(--sortie); }
  .marge-poignee:hover::after, .marge-poignee:focus-visible::after { background: var(--rose-fonce); }
  .marge-poignee:focus-visible { outline: none; }
  .marge-voile { display: none; }
  .marge-corps :global(.panel-side) { position: static; width: auto; box-shadow: none; background: transparent; z-index: auto; }
  .marge-corps :global(.panel-side > header) { display: none; }
  .marge-corps :global(.panel-body) { padding: var(--espace-1); }

  @media (min-width: 900.02px) and (max-width: 1199.98px) {
    .marge { position: absolute; z-index: var(--z-barre); top: 0; right: 0; bottom: 0; width: min(var(--marge-l, 320px), 92vw); box-shadow: var(--o3); border-radius: var(--r-xl) 0 0 var(--r-xl); overflow: hidden; animation: marge-entre var(--d-l) var(--sortie); }
  }
  @media (max-width: 900px) {
    .marge { position: fixed; z-index: calc(var(--z-barre) + 2); left: 0; right: 0; bottom: 0; height: min(78dvh, calc(var(--vvh, 100dvh) - 48px)); border-radius: var(--r-xl) var(--r-xl) 0 0; box-shadow: var(--o3); overflow: hidden; animation: marge-monte var(--d-l) var(--sortie); }
    .marge-tete { padding-top: 0; }
    .marge-tete::before { content: ''; position: absolute; left: 50%; top: 6px; width: 40px; height: 4px; margin-left: -20px; border-radius: var(--r-p); background: var(--trait-fort); }
    .marge-voile { display: block; position: fixed; z-index: calc(var(--z-barre) + 1); inset: 0; border: 0; padding: 0; background: rgb(var(--ombre) / 38%); }
  }
  @media (pointer: coarse) {
    .marge-tete .v-seg > button { min-height: 44px; }
  }
  @keyframes marge-entre { from { transform: translateX(24px); opacity: 0; } }
  @keyframes marge-monte { from { transform: translateY(40%); } }
</style>
