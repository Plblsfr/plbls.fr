import { SvelteSet } from 'svelte/reactivity'

export interface DraftFile { key: number; name: string; size: number; id?: string; failed?: boolean; preview?: string; video?: boolean; file?: File }

const TEXT = 'estompe.draft.'
export const MAX_DRAFT_BYTES = 100 * 1024 * 1024
const MAX_AGE = 14 * 24 * 3600 * 1000

// Pièces jointes de la session : elles survivent au changement de salon (le composeur est recréé), envois en cours compris.
const session = new Map<string, DraftFile[]>()
export const sessionFiles = (key: string) => session.get(key)
export const keepSession = (key: string, files: DraftFile[]) => { session.set(key, files) }

/** Conversations ayant un brouillon (texte ou pièces jointes), pour l'indicateur de la barre latérale. */
export const draftKeys = new SvelteSet<string>()
export const markDraft = (key: string, has: boolean) => { if (has) draftKeys.add(key); else draftKeys.delete(key) }

function textKeys(): string[] {
  try { return Object.keys(localStorage).filter((k) => k.startsWith(TEXT) && localStorage.getItem(k)).map((k) => k.slice(TEXT.length)) } catch { return [] }
}

// Les fichiers du brouillon sont gardés localement (IndexedDB), jamais envoyés avant « Envoyer » ; renvoyés au redémarrage.
interface Row { files: File[]; at: number }
function idb<T>(run: (s: IDBObjectStore) => IDBRequest<T>, mode: IDBTransactionMode = 'readonly'): Promise<T | undefined> {
  return new Promise((resolve) => {
    try {
      const open = indexedDB.open('estompe-drafts', 1)
      open.onupgradeneeded = () => open.result.createObjectStore('files')
      open.onerror = () => resolve(undefined)
      open.onsuccess = () => {
        const req = run(open.result.transaction('files', mode).objectStore('files'))
        req.onsuccess = () => resolve(req.result)
        req.onerror = () => resolve(undefined)
      }
    } catch { resolve(undefined) }
  })
}

/** Enregistre les fichiers du brouillon ; `false` s'ils sont trop lourds pour être conservés après un redémarrage. */
export async function persistFiles(key: string, files: File[]): Promise<boolean> {
  if (!files.length) { await idb((s) => s.delete(key), 'readwrite'); return true }
  if (files.reduce((n, f) => n + f.size, 0) > MAX_DRAFT_BYTES) { await idb((s) => s.delete(key), 'readwrite'); return false }
  const row: Row = { files, at: Date.now() }
  return (await idb((s) => s.put(row, key), 'readwrite')) !== undefined
}

export async function restoreFiles(key: string): Promise<File[]> {
  const row = (await idb((s) => s.get(key))) as Row | undefined
  return row && Date.now() - row.at < MAX_AGE ? row.files : []
}

export function clearDraft(key: string) {
  session.delete(key)
  void persistFiles(key, [])
}

/** Au démarrage : repère les brouillons de fichiers et supprime ceux de plus de 14 jours. */
export async function loadDrafts() {
  textKeys().forEach((k) => draftKeys.add(k))
  const keys = ((await idb((s) => s.getAllKeys())) ?? []) as string[]
  for (const k of keys) {
    if ((await restoreFiles(k)).length) draftKeys.add(k)
    else void persistFiles(k, [])
  }
}
