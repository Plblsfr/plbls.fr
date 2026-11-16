import type { FragmentLoaderConstructor, FragmentLoaderContext, HlsConfig, Loader } from 'hls.js'
import { mergeRanges, type BufferPref, type Range } from '../../sections/entracte/films'

export interface Frag { url: string; start: number; duration: number }
export interface Budget { seconds: number; bytes: number }
export type Fetcher = (url: string) => Promise<Blob>

const GIO = 1024 ** 3
export const BUDGETS: Record<BufferPref, Budget | null> = {
  normal: null,
  long: { seconds: 15 * 60, bytes: 2 * GIO },
  all: { seconds: Number.POSITIVE_INFINITY, bytes: 16 * GIO },
}
const KEEP_BEHIND = 30
const RETRY_MS = 5000
const MAX_FAILURES = 3

const download: Fetcher = async (url) => {
  const r = await fetch(url)
  if (!r.ok) throw new Error(`HTTP ${r.status}`)
  return r.blob()
}

export class Avance {
  private frags: Frag[] = []
  private cache = new Map<string, Blob>()
  private flying = new Map<string, Promise<Blob>>()
  private size = 0
  private running = false
  private gone = false
  private failures = 0
  private retry: ReturnType<typeof setTimeout> | undefined

  constructor(private budget: Budget, private position: () => number, private get: Fetcher = download) {}

  get bytes() { return this.size }

  setFragments(frags: Frag[]) {
    this.frags = frags
    this.wake()
  }

  fetch(url: string): Promise<Blob> {
    const hit = this.cache.get(url)
    if (hit) return Promise.resolve(hit)
    let p = this.flying.get(url)
    if (!p) {
      p = this.get(url).then((b) => {
        if (!this.gone) { this.cache.set(url, b); this.size += b.size }
        return b
      }).finally(() => this.flying.delete(url))
      this.flying.set(url, p)
    }
    return p
  }

  ranges(): Range[] {
    return mergeRanges(this.frags.filter((f) => this.cache.has(f.url)).map((f): Range => [f.start, f.start + f.duration]))
  }

  wake() {
    if (this.running || this.gone || this.failures >= MAX_FAILURES) return
    this.running = true
    void this.run().finally(() => { this.running = false })
  }

  destroy() {
    this.gone = true
    clearTimeout(this.retry)
    this.cache.clear()
    this.size = 0
  }

  private next(): Frag | undefined {
    const pos = this.position()
    return this.frags.find((f) => f.start + f.duration > pos && f.start <= pos + this.budget.seconds && !this.cache.has(f.url))
  }

  private evict(): boolean {
    const limit = this.position() - KEEP_BEHIND
    for (const f of this.frags) {
      if (this.size < this.budget.bytes) return true
      if (f.start + f.duration > limit) break
      const b = this.cache.get(f.url)
      if (b) { this.cache.delete(f.url); this.size -= b.size }
    }
    return this.size < this.budget.bytes
  }

  private async run() {
    while (!this.gone) {
      const f = this.next()
      if (!f || !this.evict()) return
      try {
        await this.fetch(f.url)
        this.failures = 0
      } catch {
        this.failures++
        clearTimeout(this.retry)
        if (!this.gone && this.failures < MAX_FAILURES) this.retry = setTimeout(() => this.wake(), RETRY_MS)
        return
      }
    }
  }
}

export function cachedLoader(Base: new (c: HlsConfig) => Loader<FragmentLoaderContext>, avance: Avance): FragmentLoaderConstructor {
  return class implements Loader<FragmentLoaderContext> {
    private inner: Loader<FragmentLoaderContext>
    private off = false
    constructor(c: HlsConfig) { this.inner = new Base(c) }
    get context() { return this.inner.context }
    get stats() { return this.inner.stats }
    load(...[context, config, callbacks]: Parameters<Loader<FragmentLoaderContext>['load']>) {
      if (context.rangeStart || context.rangeEnd) { this.inner.load(context, config, callbacks); return }
      const t0 = performance.now()
      avance.fetch(context.url).then((b) => b.arrayBuffer()).then((data) => {
        if (this.off) return
        const s = this.inner.stats
        s.loading.start = t0
        s.loading.first = s.loading.end = Math.max(performance.now(), t0 + 1)
        s.loaded = s.total = data.byteLength
        callbacks.onSuccess({ url: context.url, data }, s, context, null)
      }, () => { if (!this.off) this.inner.load(context, config, callbacks) })
    }
    abort() { this.off = true; this.inner.abort() }
    destroy() { this.off = true; this.inner.destroy() }
  }
}
