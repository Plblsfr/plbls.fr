/** Envoi d'un replay Bobyn dans une conversation (EST-128) : réencodage éventuel, envoi avec reprise automatique, message. Continue en arrière-plan quand on change de page. */
import { api } from '../../frontend/src/lib/api'
import { app } from '../../frontend/src/lib/store.svelte'
import { notify } from '../../frontend/src/lib/ui'
import { lastViewConv } from '../../frontend/src/lib/lastview'
import { destinations, needsCompression, onExport, replayUrl, replaysCancel, replaysDelete, replaysExport, replaysList, replaysThumb, sendErrorMessage, targetKbps, withRetry, type Destination, type Replay } from './replays'

export interface SendJob {
  id: number
  name: string
  to: string
  step: 'compress' | 'upload' | 'send' | 'done' | 'error'
  pct: number
  retry: number
  error: string
}
export interface SendOptions { replay: Replay; dir: string; to: string; label: string; message: string; compress: boolean }

const MAX_THUMB = 60 * 1024
let seq = 0

/** Miniature JPEG de 320 px de large (64 Ko au plus côté serveur) ; null si elle ne peut pas être produite : la vidéo part alors sans. */
async function jpegThumb(url: string): Promise<Blob | null> {
  try {
    const img = new Image()
    img.src = url
    await img.decode()
    const c = document.createElement('canvas')
    c.width = 320
    c.height = Math.max(1, Math.round((320 * img.naturalHeight) / img.naturalWidth))
    c.getContext('2d')!.drawImage(img, 0, 0, c.width, c.height)
    const b: Blob | null = await new Promise((r) => c.toBlob(r, 'image/jpeg', 0.7))
    return b && b.size <= MAX_THUMB ? b : null
  } catch {
    return null
  }
}

class ReplaySends {
  jobs = $state<SendJob[]>([])
  private ctl = new Map<number, { abort: AbortController; exportId?: number; fileId?: string; opts: SendOptions }>()
  /** Exports lancés pour un envoi : la bibliothèque ne les annonce pas comme des exports de l'utilisateur. */
  private exports = new Set<number>()

  ownsExport = (id: number) => this.exports.has(id)

  start(opts: SendOptions) {
    this.jobs.push({ id: ++seq, name: opts.replay.name, to: opts.label, step: opts.compress ? 'compress' : 'upload', pct: 0, retry: 0, error: '' })
    const j = this.jobs[this.jobs.length - 1]
    this.ctl.set(j.id, { abort: new AbortController(), opts })
    void this.run(j.id)
    return j.id
  }

  private job = (id: number) => this.jobs.find((j) => j.id === id)

  /** Relance un envoi en erreur ; un fichier déjà envoyé au serveur n'est pas renvoyé. */
  retry(id: number) {
    const j = this.job(id)
    const c = this.ctl.get(id)
    if (!j || !c) return
    c.abort = new AbortController()
    Object.assign(j, { error: '', pct: 0, retry: 0 })
    void this.run(id)
  }

  cancel(id: number) {
    const c = this.ctl.get(id)
    c?.abort.abort()
    if (c?.exportId !== undefined) void replaysCancel(c.exportId).catch(() => {})
    this.dismiss(id)
  }

  dismiss(id: number) {
    this.jobs = this.jobs.filter((j) => j.id !== id)
    this.ctl.delete(id)
  }

  /** Réencode en 720p au débit qui fait tenir le replay sous la limite ; renvoie le nom du fichier produit dans la bibliothèque. */
  private async compress(j: SendJob, id: number): Promise<string> {
    const c = this.ctl.get(id)!
    const r = c.opts.replay
    let off = () => {}
    const done = new Promise<string>((resolve, reject) => {
      void onExport((e) => {
        if (e.id !== c.exportId) return
        if (e.state === 'run') j.pct = e.pct
        else if (e.state === 'done' && e.file) resolve(e.file)
        else reject(e.state === 'cancel' ? new DOMException('annulé', 'AbortError') : new Error(e.message ?? 'Réencodage impossible.'))
      }).then((f) => (off = f))
    })
    try {
      c.exportId = await replaysExport({ name: r.name, start: 0, end: r.duration, crop: null, quality: '720', target_kbps: targetKbps(app.maxVideo, r.duration) })
      this.exports.add(c.exportId)
      return await done
    } finally {
      off()
    }
  }

  private async run(id: number) {
    const c = this.ctl.get(id)!
    const j = this.job(id)!
    const { replay, dir, to, label, message, compress } = c.opts
    let temp = ''
    try {
      if (!c.fileId) {
        let file = replay.name
        if (compress) {
          j.step = 'compress'
          file = temp = await this.compress(j, id)
        }
        j.step = 'upload'
        j.pct = 0
        const blob = await (await fetch(await replayUrl(dir, file))).blob()
        if (blob.size > app.maxVideo) throw Object.assign(new Error(`Le fichier fait encore ${Math.round(blob.size / 1048576)} Mo (limite : ${Math.round(app.maxVideo / 1048576)} Mo).`), { status: 413 })
        const thumb = await replaysThumb(file).then(jpegThumb, () => null)
        const a = await withRetry(() => api.uploadVideo(blob, file, thumb, (s, t) => (j.pct = s / t), c.abort.signal), undefined, undefined, (n) => (j.retry = n))
        c.fileId = a.id
      }
      j.step = 'send'
      await app.send(to, message, [c.fileId])
      j.step = 'done'
      notify.ok(`Replay envoyé dans ${label}`)
      setTimeout(() => this.dismiss(id), 4000)
    } catch (e) {
      if ((e as Error)?.name === 'AbortError') return
      j.step = 'error'
      j.error = sendErrorMessage(e)
      notify.fail(`Replay non envoyé dans ${label} : ${j.error}`)
    } finally {
      if (temp) void replaysDelete(temp).catch(() => {})
    }
  }
}

export const sends = new ReplaySends()

/** Salons texte (où l'on peut écrire et joindre des fichiers) et messages privés où envoyer un replay. */
export const sendDestinations = (): Destination[] =>
  destinations(app.servers, Object.fromEntries(Object.entries(app.details).map(([id, d]) => [id, d.channels])), app.dms, (k) => app.canConv(k, 'send_messages') && app.canConv(k, 'attach_files'))

/** Dernière conversation ouverte, si l'on peut encore y envoyer un replay. */
export const lastDestination = () => {
  const key = lastViewConv(app.meId, app.servers, app.dms)
  return sendDestinations().find((d) => d.key === key) ?? null
}

/** « Sauvegarder et envoyer » : toast Windows proposant d'envoyer le replay qui vient d'être sauvegardé dans la dernière conversation ouverte. */
export async function offerSend(name: string, notifyOn: boolean) {
  if (!notifyOn) return
  const to = lastDestination()
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke('toast_show', { title: 'Replay sauvegardé', body: to ? `${name}\nEnvoyer dans ${to.label} (${to.group}) ?` : name, href: '', conv: to ? `replay:${name}` : '' }).catch(() => {})
}

/** Action « Envoyer » du toast : envoi en arrière-plan, sans message ; réencodé si le replay dépasse la limite. */
export async function quickSend(name: string) {
  const to = lastDestination()
  const lib = await replaysList().catch(() => null)
  const replay = lib?.replays.find((r) => r.name === name)
  if (!to || !lib || !replay) return notify.fail("Replay introuvable ou conversation indisponible : ouvrez la bibliothèque Bobyn pour l'envoyer.")
  sends.start({ replay, dir: lib.dir, to: to.key, label: to.label, message: '', compress: needsCompression(replay.size, app.maxVideo) })
}
