import { toast } from '../vendor/plbls-ui/plbls-ui.esm.min.js'
import { ApiError } from './api'
import { debug } from './debuglog.svelte'

const TOAST_MAX = 3
const TOAST_MS = 5000
type Toast = { kind: 'info' | 'succes' | 'erreur'; message: string; count: number; until: number }
let live: Toast[] = []
const waiting: Toast[] = []
let pumpTimer: ReturnType<typeof setTimeout> | undefined

function render() {
  toast.clear()
  for (const t of live) toast.show(t.count > 1 ? `${t.message} (×${t.count})` : t.message, t.kind, TOAST_MS)
}

function pump() {
  const now = Date.now()
  live = live.filter((t) => t.until > now)
  while (live.length < TOAST_MAX && waiting.length) live.push({ ...waiting.shift()!, until: now + TOAST_MS })
  render()
  clearTimeout(pumpTimer)
  if (live.length) pumpTimer = setTimeout(pump, Math.min(...live.map((t) => t.until)) - now + 50)
}

/** Une notification identique à une notification visible ou en attente n'en crée pas de nouvelle : son compteur augmente. Trois au plus sont visibles, les autres attendent. */
function show(kind: Toast['kind'], message: string) {
  const now = Date.now()
  live = live.filter((t) => t.until > now)
  const same = [...live, ...waiting].find((t) => t.kind === kind && t.message === message)
  if (same) {
    same.count++
    if (live.includes(same)) same.until = now + TOAST_MS
  } else if (live.length < TOAST_MAX) live.push({ kind, message, count: 1, until: now + TOAST_MS })
  else waiting.push({ kind, message, count: 1, until: 0 })
  pump()
}

export const notify = {
  ok: (m: string) => show('succes', m),
  info: (m: string) => show('info', m),
  error: (e: unknown, fallback = 'Une erreur est survenue') => {
    debug.log('error', 'interface', `${fallback} — ${e instanceof ApiError ? `${e.status} ${e.code} : ${e.message}` : String(e)}`)
    show('erreur', messageOf(e, fallback))
  },
  /** Message d'erreur déjà rédigé. */
  fail: (message: string) => show('erreur', message),
}

/** Message lisible pour l'utilisateur à partir d'une erreur de l'API. */
export function messageOf(e: unknown, fallback = 'Une erreur est survenue'): string {
  if (e instanceof ApiError) {
    if (e.status === 429 && e.code !== 'slowmode') return 'Trop de demandes, patientez quelques secondes.'
    if (e.status === 413) return 'Fichier trop gros (10 Mo maximum).'
    if (e.status === 0 || e.status >= 500) return fallback
    return e.message || fallback
  }
  if (e instanceof TypeError) return 'Connexion au serveur impossible.'
  return fallback
}
