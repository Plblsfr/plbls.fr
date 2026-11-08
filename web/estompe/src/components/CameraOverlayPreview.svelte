<script lang="ts">
  import type { LocalVideoTrack } from 'livekit-client'
  import { applyCameraFx } from '../lib/camerafx'
  import { openCamera, overlayRect, type Composer } from '../lib/camoverlay'
  import { prefs } from '../lib/prefs.svelte'

  let cv = $state<HTMLCanvasElement>()
  let comp = $state<Composer>()
  let error = $state('')

  // Même chemin que le partage : caméra (avec son effet) composée sur un faux écran.
  $effect(() => {
    void [prefs.cameraFx, prefs.cameraBlur, prefs.cameraBg, prefs.customBg]
    let dead = false
    let stream: MediaStream | undefined
    let lk: LocalVideoTrack | undefined
    let close: (() => void) | undefined
    error = ''
    ;(async () => {
      stream = await navigator.mediaDevices.getUserMedia({ video: { deviceId: prefs.cameraId ? { exact: prefs.cameraId } : undefined, width: { ideal: 1280 }, height: { ideal: 720 } } })
      if (dead) return
      const { LocalVideoTrack } = await import('livekit-client')
      lk = new LocalVideoTrack(stream.getVideoTracks()[0])
      await applyCameraFx(lk, () => { prefs.cameraFx = 'none'; prefs.save() })
      const c = await openCamera(lk)
      if (dead) return c.close()
      close = c.close
      comp = c.comp
    })().catch(() => (error = 'Caméra indisponible ou accès refusé.'))
    return () => { dead = true; comp = undefined; close?.(); lk?.stop(); stream?.getTracks().forEach((t) => t.stop()) }
  })

  $effect(() => {
    const c = comp
    const ctx = cv?.getContext('2d')
    if (!c || !cv || !ctx) return
    let raf = 0
    const loop = () => {
      const { width: w, height: h } = cv!
      const g = ctx.createLinearGradient(0, 0, w, h)
      g.addColorStop(0, '#2b2f4f')
      g.addColorStop(1, '#4a3a5c')
      ctx.fillStyle = g
      ctx.fillRect(0, 0, w, h)
      ctx.fillStyle = 'rgba(255,255,255,.35)'
      ctx.font = '20px sans-serif'
      ctx.fillText("Votre partage d'écran", 24, 40)
      c.draw(ctx, w, h)
      raf = requestAnimationFrame(loop)
    }
    loop()
    return () => cancelAnimationFrame(raf)
  })

  function drag(e: PointerEvent) {
    if (!cv || !comp || !(e.buttons & 1)) return
    const b = cv.getBoundingClientRect()
    const o = prefs.camOverlay
    const r = overlayRect(o, cv.width, cv.height, 16 / 9)
    const m = Math.min(o.margin, Math.max(0, (cv.width - r.w) / 2), Math.max(0, (cv.height - r.h) / 2))
    const px = ((e.clientX - b.left) / b.width) * cv.width - r.w / 2 - m
    const py = ((e.clientY - b.top) / b.height) * cv.height - r.h / 2 - m
    o.x = Math.min(1, Math.max(0, px / (cv.width - r.w - 2 * m)))
    o.y = Math.min(1, Math.max(0, py / (cv.height - r.h - 2 * m)))
  }
</script>

<canvas bind:this={cv} width="640" height="360" aria-label="Aperçu de l'incrustation : glisser pour déplacer la caméra"
  onpointerdown={(e) => { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); drag(e) }} onpointermove={drag} onpointerup={() => prefs.save()}></canvas>
{#if error}<p class="v-alert dgr ov-err" role="alert">{error}</p>{/if}

<style>
  canvas { display: block; width: 100%; aspect-ratio: 16 / 9; border-radius: var(--r-l); background: var(--papier-3); box-shadow: var(--o1); touch-action: none; cursor: grab; }
  canvas:active { cursor: grabbing; }
  .ov-err { margin: 0; }
</style>
