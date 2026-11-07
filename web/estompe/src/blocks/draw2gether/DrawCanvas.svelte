<script lang="ts">
  import { draw } from './draw.svelte'
  import { BATCH, CANVAS_H, CANVAS_W, ERASER_COLOR, type Trace } from './draw'

  let { channelId, canDraw, tool, color, size }: { channelId: string; canDraw: boolean; tool: 'pen' | 'eraser' | 'fill'; color: string; size: number } = $props()

  const g = $derived(draw.games[channelId])
  let canvas = $state<HTMLCanvasElement>()
  let epoch = -1
  let drawn = 0
  let lastLen = 0
  let current: { id: string; pending: [number, number][] } | null = null

  function paint(ctx: CanvasRenderingContext2D, t: Trace, from: number) {
    if (t.tool === 'fill') { if (from === 0) flood(ctx, t.pts[0], t.color); return }
    ctx.strokeStyle = t.color
    ctx.fillStyle = t.color
    ctx.lineWidth = t.size
    ctx.lineCap = 'round'
    ctx.lineJoin = 'round'
    const pts = t.pts
    if (pts.length === 1 && from === 0) {
      ctx.beginPath()
      ctx.arc(pts[0][0] * CANVAS_W, pts[0][1] * CANVAS_H, t.size / 2, 0, Math.PI * 2)
      ctx.fill()
      return
    }
    ctx.beginPath()
    const start = Math.max(0, from - 1)
    ctx.moveTo(pts[start][0] * CANVAS_W, pts[start][1] * CANVAS_H)
    for (let i = start + 1; i < pts.length; i++) ctx.lineTo(pts[i][0] * CANVAS_W, pts[i][1] * CANVAS_H)
    ctx.stroke()
  }

  function flood(ctx: CanvasRenderingContext2D, p: [number, number], hex: string) {
    const x0 = Math.min(CANVAS_W - 1, Math.floor(p[0] * CANVAS_W))
    const y0 = Math.min(CANVAS_H - 1, Math.floor(p[1] * CANVAS_H))
    const img = ctx.getImageData(0, 0, CANVAS_W, CANVAS_H)
    const d = img.data
    const at = (x: number, y: number) => (y * CANVAS_W + x) * 4
    const i0 = at(x0, y0)
    const base = [d[i0], d[i0 + 1], d[i0 + 2]]
    const fill = [parseInt(hex.slice(1, 3), 16), parseInt(hex.slice(3, 5), 16), parseInt(hex.slice(5, 7), 16)]
    if (base.every((v, k) => Math.abs(v - fill[k]) < 4)) return
    const near = (i: number) => Math.abs(d[i] - base[0]) + Math.abs(d[i + 1] - base[1]) + Math.abs(d[i + 2] - base[2]) < 60
    const seen = new Uint8Array(CANVAS_W * CANVAS_H)
    const stack = [x0, y0]
    while (stack.length) {
      const y = stack.pop()!
      const x = stack.pop()!
      const k = y * CANVAS_W + x
      if (seen[k]) continue
      seen[k] = 1
      const i = k * 4
      if (!near(i)) continue
      d[i] = fill[0]; d[i + 1] = fill[1]; d[i + 2] = fill[2]; d[i + 3] = 255
      if (x > 0) stack.push(x - 1, y)
      if (x < CANVAS_W - 1) stack.push(x + 1, y)
      if (y > 0) stack.push(x, y - 1)
      if (y < CANVAS_H - 1) stack.push(x, y + 1)
    }
    ctx.putImageData(img, 0, 0)
  }

  $effect(() => {
    const cur = g
    if (!canvas || !cur) return
    void cur.v
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const strokes = cur.strokes
    if (cur.epoch !== epoch) {
      ctx.fillStyle = ERASER_COLOR
      ctx.fillRect(0, 0, CANVAS_W, CANVAS_H)
      strokes.forEach((t) => paint(ctx, t, 0))
      epoch = cur.epoch
    } else {
      for (let i = Math.max(0, drawn - 1); i < strokes.length; i++) paint(ctx, strokes[i], i === drawn - 1 ? lastLen : 0)
    }
    drawn = strokes.length
    lastLen = strokes.at(-1)?.pts.length ?? 0
  })

  function point(e: PointerEvent): [number, number] {
    const r = canvas!.getBoundingClientRect()
    const clamp = (v: number) => Math.min(1, Math.max(0, v))
    return [clamp((e.clientX - r.left) / r.width), clamp((e.clientY - r.top) / r.height)]
  }

  const base = () => ({ op: 'stroke' as const, tool, color: tool === 'eraser' ? ERASER_COLOR : color, size })

  function flush() {
    if (!current || !current.pending.length) return
    draw.send(channelId, { ...base(), id: current.id, pts: current.pending })
    current.pending = []
  }

  function down(e: PointerEvent) {
    if (!canDraw || e.button > 0) return
    e.preventDefault()
    canvas!.setPointerCapture(e.pointerId)
    const p = point(e)
    const id = Math.random().toString(36).slice(2, 14)
    draw.apply(channelId, { ...base(), id, pts: [p] })
    if (tool === 'fill') { draw.send(channelId, { ...base(), id, pts: [p] }); return }
    current = { id, pending: [p] }
  }

  function move(e: PointerEvent) {
    if (!current) return
    const p = point(e)
    draw.apply(channelId, { ...base(), id: current.id, pts: [p] })
    current.pending.push(p)
    if (current.pending.length >= BATCH) flush()
  }

  function up() {
    flush()
    current = null
  }
</script>

<canvas bind:this={canvas} width={CANVAS_W} height={CANVAS_H} class:can={canDraw} aria-label="Zone de dessin" onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}></canvas>

<style>
  canvas { display: block; width: 100%; height: auto; aspect-ratio: 4 / 3; background: #ffffff; border-radius: var(--r-l); box-shadow: inset 0 0 0 1px var(--trait), 0 0 0 1px var(--trait); touch-action: none; }
  canvas.can { cursor: crosshair; }
</style>
