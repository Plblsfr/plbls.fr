import { api } from '../../lib/api'
import { platform } from '../../lib/platform'
import { app } from '../../lib/store.svelte'
import type { Attachment } from '../../lib/types'
import { checkSize, prepareUpload } from '../../lib/files'

const MAX_THUMB = 60 * 1024

/** Miniature JPEG de 320 px (64 Ko au plus côté serveur) prise une seconde après le début ; null si le navigateur ne lit pas la vidéo : elle part alors sans. */
export async function videoThumb(file: Blob): Promise<Blob | null> {
  const url = URL.createObjectURL(file)
  try {
    const v = document.createElement('video')
    v.muted = true
    v.preload = 'auto'
    v.src = url
    await new Promise<void>((ok, ko) => { v.onloadeddata = () => ok(); v.onerror = () => ko(new Error('illisible')) })
    v.currentTime = Math.min(1, (v.duration || 0) / 2)
    await new Promise<void>((ok, ko) => { v.onseeked = () => ok(); v.onerror = () => ko(new Error('illisible')) })
    const c = document.createElement('canvas')
    c.width = 320
    c.height = Math.max(1, Math.round((320 * v.videoHeight) / v.videoWidth))
    c.getContext('2d')!.drawImage(v, 0, 0, c.width, c.height)
    const b: Blob | null = await new Promise((r) => c.toBlob(r, 'image/jpeg', 0.7))
    return b && b.size <= MAX_THUMB ? b : null
  } catch {
    return null
  } finally {
    URL.revokeObjectURL(url)
  }
}

const ascii = (b: Uint8Array, at: number, n = 4) => String.fromCharCode(...b.subarray(at, at + n))

/** Type de l'échantillon vidéo d'un `moov` (`avc1`, `hvc1`, `hev1`, `av01`…) ; null s'il n'y a pas de piste vidéo. */
export function codecOfMoov(moov: Uint8Array): string | null {
  for (let i = 0; i + 8 <= moov.length; i++) {
    if (ascii(moov, i) !== 'vide') continue
    for (let j = i; j + 20 <= moov.length; j++) if (ascii(moov, j) === 'stsd') return ascii(moov, j + 16)
    return null
  }
  return null
}

/** Codec vidéo d'un MP4 ; lit seulement les en-têtes des boîtes puis le `moov` (en tête ou en fin de fichier). null si illisible. */
export async function mp4Codec(file: Blob): Promise<string | null> {
  const MAX_MOOV = 16 << 20
  for (let off = 0; off + 8 <= file.size; ) {
    const h = new DataView(await file.slice(off, off + 16).arrayBuffer())
    let size = h.getUint32(0)
    let head = 8
    if (size === 1) { size = Number(h.getBigUint64(8)); head = 16 } else if (size === 0) size = file.size - off
    if (size < head) return null
    if (String.fromCharCode(h.getUint8(4), h.getUint8(5), h.getUint8(6), h.getUint8(7)) === 'moov') {
      return size > MAX_MOOV ? null : codecOfMoov(new Uint8Array(await file.slice(off + head, off + size).arrayBuffer()))
    }
    off += size
  }
  return null
}

/**
 * Envoie une vidéo MP4 jointe à un message, avec sa miniature et sa durée (lue par le serveur). Si la plateforme sait convertir (app Windows), un codec autre que
 * H.264 (HEVC, AV1 : souvent illisibles dans un navigateur) est converti d'abord ; sans encodeur, l'original part tel quel.
 */
export async function uploadVideoFile(file: File): Promise<Attachment> {
  let blob: Blob = file
  if (platform.toH264 && (await mp4Codec(file)) !== 'avc1') blob = await platform.toH264(file).catch(() => file)
  if (blob.size > app.maxVideo) throw new Error(`Vidéo trop grosse (${Math.round(app.maxVideo / 1048576)} Mo maximum)`)
  return api.uploadVideo(blob, file.name, await videoThumb(blob), () => {})
}

/** Envoie un fichier (compression des images, vidéo en deux temps) ; lève une erreur si trop gros ou refusé. */
export async function uploadAttachment(file: File): Promise<Attachment> {
  if (file.type === 'video/mp4') return uploadVideoFile(file)
  const { blob, name } = await prepareUpload(file)
  const tooBig = checkSize(blob.size)
  if (tooBig) throw new Error(tooBig)
  return api.upload(blob, name)
}
