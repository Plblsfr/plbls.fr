import type { FragmentLoaderConstructor, FragmentLoaderContext, HlsConfig, Loader } from 'hls.js'
import { mergeRanges, type BufferPref, type Range } from '../../sections/entracte/films'
import { memoire, type Depot } from './depot'

export interface Frag { url: string; start: number; duration: number }
export interface Budget { seconds: number; bytes: number }
export interface Demande { prefetch: boolean; signal: AbortSignal }
export type Fetcher = (url: string, demande: Demande) => Promise<Blob>
export interface Options { depot?: Depot; parallel?: number }

const GIO = 1024 ** 3
export const BUDGETS: Record<BufferPref, Budget | null> = {
  normal: null,
  long: { seconds: 15 * 60, bytes: 2 * GIO },
  all: { seconds: Number.POSITIVE_INFINITY, bytes: 16 * GIO },
}
export const PARALLELE = 3
const KEEP_BEHIND = 30
const RETRY_MS = 5000
const MAX_FAILURES = 3

export class PasPret extends Error {}

export const nomDeSegment = (url: string) => url.split(/[?#]/)[0].split('/').pop() || url

const download: Fetcher = async (url, { prefetch, signal }) => {
  const r = await fetch(prefetch ? `${url}${url.includes('?') ? '&' : '?'}avance=1` : url, { signal })
  if (prefetch && r.status === 404) throw new PasPret()
  if (!r.ok) throw new Error(`HTTP ${r.status}`)
  return r.blob()
}

interface Vol { promise: Promise<Blob>; stop: AbortController }

export class Avance {
  private frags: Frag[] = []
  private flying = new Map<string, Vol>()
  private depot: Depot
  private owned: boolean
  private healthy = false
  private parallel: number
  private budget: () => Budget | null
  private prefetching = 0
  private gone = false
  private failures = 0
  private retry: ReturnType<typeof setTimeout> | undefined
  private waiting: string | null = null
  private hold: ReturnType<typeof setTimeout> | undefined

  constructor(budget: Budget | (() => Budget | null), private position: () => number, private get: Fetcher = download, options: Options = {}) {
    this.budget = typeof budget === 'function' ? budget : () => budget
    this.owned = !options.depot
    this.depot = options.depot ?? memoire()
    this.parallel = options.parallel ?? PARALLELE
  }

  get bytes() { return this.depot.bytes() }

  get active() { return !this.gone && this.budget() !== null }

  has(url: string) { return this.depot.has(nomDeSegment(url)) }

  setFragments(frags: Frag[]) {
    this.frags = frags
    this.wake()
  }

  async fetch(url: string): Promise<Blob> {
    const name = nomDeSegment(url)
    if (this.depot.has(name)) {
      const kept = await this.depot.get(name)
      if (kept) return kept
    }
    const vol = this.flying.get(name)
    if (vol) return vol.promise.catch(() => this.load(url, name, false))
    return this.load(url, name, false)
  }

  ranges(): Range[] {
    return mergeRanges(this.frags.filter((f) => this.has(f.url)).map((f): Range => [f.start, f.start + f.duration]))
  }

  wake() {
    if (this.gone || this.retry !== undefined || this.failures >= MAX_FAILURES) return
    this.pump()
  }

  destroy() {
    this.gone = true
    clearTimeout(this.retry)
    clearTimeout(this.hold)
    for (const v of this.flying.values()) v.stop.abort()
    this.flying.clear()
    this.frags = []
    if (this.owned) void this.depot.close(true)
  }

  private load(url: string, name: string, prefetch: boolean): Promise<Blob> {
    const stop = new AbortController()
    const promise = this.get(url, { prefetch, signal: stop.signal }).then(async (b) => {
      if (this.active) await this.depot.put(name, b)
      return b
    }).finally(() => { if (this.flying.get(name)?.promise === promise) this.flying.delete(name) })
    this.flying.set(name, { promise, stop })
    return promise
  }

  private next(b: Budget): Frag | undefined {
    const pos = this.position()
    return this.frags.find((f) => {
      const name = nomDeSegment(f.url)
      return f.start + f.duration > pos && f.start <= pos + b.seconds && !this.depot.has(name) && !this.flying.has(name)
    })
  }

  private evict(b: Budget): boolean {
    const kept = this.depot.names()
    const reserve = kept.length ? (this.depot.bytes() / kept.length) * this.prefetching : 0
    const room = () => this.depot.bytes() + reserve < b.bytes
    if (room()) return true
    const listed = new Set(this.frags.map((f) => nomDeSegment(f.url)))
    for (const name of kept) {
      if (room()) return true
      if (!listed.has(name)) this.depot.drop(name)
    }
    const limit = this.position() - KEEP_BEHIND
    for (const f of this.frags) {
      if (room()) return true
      if (f.start + f.duration > limit) break
      this.depot.drop(nomDeSegment(f.url))
    }
    return room()
  }

  private pump() {
    const b = this.gone ? null : this.budget()
    if (!b) return
    const limit = this.healthy ? this.parallel : 1
    while (this.prefetching < limit) {
      const f = this.next(b)
      if (!f || !this.evict(b)) return
      const name = nomDeSegment(f.url)
      if (name === this.waiting) return
      this.prefetching++
      this.load(f.url, name, true).then(() => {
        this.prefetching--
        this.failures = 0
        this.healthy = true
        this.wake()
      }, (e: unknown) => {
        this.prefetching--
        if (this.gone) return
        if (e instanceof PasPret) { this.wait(name); return }
        this.healthy = false
        if (this.retry !== undefined) return
        this.failures++
        if (this.failures < MAX_FAILURES) this.retry = setTimeout(() => { this.retry = undefined; this.wake() }, RETRY_MS)
      })
    }
  }

  private wait(name: string) {
    this.waiting = name
    clearTimeout(this.hold)
    this.hold = setTimeout(() => { this.waiting = null; this.wake() }, RETRY_MS)
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
      if (context.rangeStart || context.rangeEnd || !(avance.active || avance.has(context.url))) { this.inner.load(context, config, callbacks); return }
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
