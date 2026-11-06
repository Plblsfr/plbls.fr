// Mode debug (EST-48) : journal des actions, erreurs et évènements internes. Rien n'est enregistré tant qu'il est désactivé.
// Les lignes vont en mémoire (vue en direct) et, dans les apps Windows et Android, dans un fichier géré par Rust (rotation). Jamais de contenu de message.
import { platform } from './platform'
import { prefs } from './prefs.svelte'

export type Level = 'info' | 'warn' | 'error'
export interface Entry { t: number; l: Level; s: string; m: string }

const MAX_ENTRIES = 2000

/** Masque ce qui ressemble à un secret : jetons JWT, en-têtes Bearer, paramètres ou champs token / mot de passe. */
export function redact(s: string): string {
  return s
    .replace(/eyJ[\w-]+\.[\w-]+\.[\w-]*/g, '[jeton]')
    .replace(/\bBearer\s+[\w.~+/=-]+/gi, 'Bearer [jeton]')
    .replace(/((?:token|password|passwd|mot_de_passe|secret|code|authorization|apikey|api_key)["']?\s*[:=]\s*)(?:"[^"]*"|'[^']*'|[^\s&,;}]+)/gi, '$1[masqué]')
}

export function filterEntries(list: Entry[], level: Level | 'all', query: string): Entry[] {
  const q = query.trim().toLowerCase()
  return list.filter((e) => (level === 'all' || e.l === level) && (!q || `${e.s} ${e.m}`.toLowerCase().includes(q)))
}

const pad = (n: number, w = 2) => String(n).padStart(w, '0')
export function formatEntry(e: Entry): string {
  const d = new Date(e.t)
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)} [${e.l.toUpperCase()}] ${e.s}: ${e.m}`
}

export function parseLines(raw: string): Entry[] {
  const out: Entry[] = []
  for (const line of raw.split('\n')) {
    try {
      const e = JSON.parse(line)
      if (typeof e.t === 'number' && typeof e.m === 'string') out.push({ t: e.t, l: e.l, s: e.s, m: e.m })
    } catch { /* ligne tronquée ou vide */ }
  }
  return out
}

const text = (a: unknown[]) => a.map((x) => (x instanceof Error ? x.message : typeof x === 'string' ? x : (() => { try { return JSON.stringify(x) } catch { return String(x) } })())).join(' ')

class DebugLog {
  enabled = $state(false)
  entries = $state<Entry[]>([])
  private off: (() => void) | null = null

  /** Au démarrage : rétablit l'état enregistré. */
  start() { if (prefs.debug) void this.setEnabled(true, false) }

  async setEnabled(on: boolean, persist = true) {
    if (on === this.enabled) return
    if (persist) { prefs.debug = on; prefs.save() }
    if (on) {
      this.enabled = true
      this.hook()
      const store = platform.debugStore
      if (store) {
        await store.set(true)
        const past = parseLines(await store.read())
        this.entries = [...past, ...this.entries].sort((a, b) => a.t - b.t).slice(-MAX_ENTRIES)
      }
      this.log('info', 'debug', 'mode debug activé')
    } else {
      this.log('info', 'debug', 'mode debug désactivé')
      this.enabled = false
      this.off?.()
      this.off = null
      await platform.debugStore?.set(false)
    }
  }

  log(level: Level, source: string, msg: string) {
    if (!this.enabled) return
    const e: Entry = { t: Date.now(), l: level, s: source, m: redact(msg) }
    this.entries.push(e)
    if (this.entries.length > MAX_ENTRIES) this.entries.splice(0, this.entries.length - MAX_ENTRIES)
    platform.debugStore?.write(JSON.stringify(e))
  }

  async clear() {
    this.entries = []
    await platform.debugStore?.clear()
  }

  /** Erreurs non attrapées et console.warn / console.error, tant que le mode est actif. */
  private hook() {
    const onError = (ev: ErrorEvent) => this.log('error', 'interface', ev.message)
    const onReject = (ev: PromiseRejectionEvent) => this.log('error', 'interface', `promesse rejetée : ${text([ev.reason])}`)
    addEventListener('error', onError)
    addEventListener('unhandledrejection', onReject)
    const { warn, error } = console
    console.warn = (...a) => { this.log('warn', 'console', text(a)); warn(...a) }
    console.error = (...a) => { this.log('error', 'console', text(a)); error(...a) }
    this.off = () => {
      removeEventListener('error', onError)
      removeEventListener('unhandledrejection', onReject)
      console.warn = warn
      console.error = error
    }
  }
}

export const debug = new DebugLog()
