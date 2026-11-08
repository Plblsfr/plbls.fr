<script lang="ts">
  import Modal from './Modal.svelte'
  import Icon from './Icon.svelte'
  import { cropRect, squareAvatar, type Crop } from '../blocks/avatars/avatars.svelte'
  import { notify } from '../lib/ui'

  let { file, onconfirm, onclose }: { file: Blob; onconfirm: (img: Blob) => void; onclose: () => void } = $props()

  const SIDE = 240
  let canvas = $state<HTMLCanvasElement>()
  let crop = $state<Crop>({ zoom: 1, x: 0, y: 0 })
  let bmp = $state<ImageBitmap>()
  let drag: { px: number; py: number } | null = null

  $effect(() => {
    let alive = true
    createImageBitmap(file).then((b) => { if (alive) bmp = b }, () => { notify.fail('Image illisible'); onclose() })
    return () => { alive = false }
  })
  $effect(() => {
    if (!bmp || !canvas) return
    const { sx, sy, size } = cropRect(bmp.width, bmp.height, crop)
    canvas.getContext('2d')!.drawImage(bmp, sx, sy, size, size, 0, 0, SIDE, SIDE)
  })

  // Faire glisser déplace l'image : le sens du mouvement est inversé par rapport à la fenêtre de recadrage.
  function move(e: PointerEvent) {
    if (!drag || !bmp) return
    const { size } = cropRect(bmp.width, bmp.height, crop)
    const k = (2 * size) / SIDE
    const dx = ((e.clientX - drag.px) * k) / Math.max(1, bmp.width - size)
    const dy = ((e.clientY - drag.py) * k) / Math.max(1, bmp.height - size)
    crop.x = Math.max(-1, Math.min(1, crop.x - dx))
    crop.y = Math.max(-1, Math.min(1, crop.y - dy))
    drag = { px: e.clientX, py: e.clientY }
  }
  async function ok() {
    try { onconfirm(await squareAvatar(file, 256, crop)) } catch (e) { notify.error(e, 'Image illisible') }
  }
</script>

<Modal title="Recadrer l'icône" {onclose}>
  <div class="ic">
    <canvas class="ic-c" bind:this={canvas} width={SIDE} height={SIDE} aria-label="Aperçu du recadrage : faire glisser pour déplacer"
      onpointerdown={(e) => { drag = { px: e.clientX, py: e.clientY }; e.currentTarget.setPointerCapture(e.pointerId) }} onpointermove={move} onpointerup={() => (drag = null)}></canvas>
    <label class="ic-zoom"><span class="v-mute"><Icon name="cherche" size={16} /></span><span class="v-lbl">Zoom</span><input class="grow" type="range" min="1" max="4" step="0.05" bind:value={crop.zoom} aria-label="Zoom" /><span class="v-mono">{crop.zoom.toLocaleString('fr-FR', { maximumFractionDigits: 1, minimumFractionDigits: 1 })}×</span></label>
    <div class="ic-f">
      <button class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button class="plbls-button" onclick={ok}>Valider</button>
    </div>
  </div>
</Modal>

<style>
  .ic { display: flex; flex-direction: column; align-items: center; gap: 16px; }
  .ic-c { width: min(240px, 100%); aspect-ratio: 1; border-radius: 50%; touch-action: none; cursor: grab; box-shadow: 0 0 0 3px var(--surface), 0 0 0 4px var(--encre); }
  .ic-c:active { cursor: grabbing; }
  .ic-zoom { display: flex; align-items: center; gap: 8px; width: 100%; }
  .ic-zoom input { accent-color: var(--rose-fonce); min-width: 0; }
  .ic-zoom :global(svg) { display: block; }
  .ic-f { display: flex; gap: 8px; justify-content: flex-end; width: 100%; }
</style>
