export interface Depot {
  has(name: string): boolean
  get(name: string): Promise<Blob | null>
  put(name: string, b: Blob): Promise<void>
  drop(name: string): void
  names(): string[]
  bytes(): number
  close(purge: boolean): Promise<void>
}

interface Ecriture { write(b: Blob): Promise<void>; close(): Promise<void> }
interface Fichier { kind: string; getFile(): Promise<Blob & { lastModified: number }>; createWritable(): Promise<Ecriture> }
export interface Dossier {
  kind: string
  getDirectoryHandle(name: string, options?: { create?: boolean }): Promise<Dossier>
  getFileHandle(name: string, options?: { create?: boolean }): Promise<Fichier>
  removeEntry(name: string, options?: { recursive?: boolean }): Promise<void>
  entries(): AsyncIterable<[string, { kind: string }]>
}

export const RACINE = 'entracte-avance'
export const RESTES_MS = 24 * 3600 * 1000
const MARQUE = '.vu'

export function memoire(): Depot {
  const kept = new Map<string, Blob>()
  let size = 0
  return {
    has: (name) => kept.has(name),
    get: async (name) => kept.get(name) ?? null,
    put: async (name, b) => { size += b.size - (kept.get(name)?.size ?? 0); kept.set(name, b) },
    drop: (name) => { size -= kept.get(name)?.size ?? 0; kept.delete(name) },
    names: () => [...kept.keys()],
    bytes: () => size,
    close: async () => { kept.clear(); size = 0 },
  }
}

export const dossierDe = (cle: string) => cle.replace(/[^A-Za-z0-9_-]/g, '_').slice(0, 120)

export async function racineDuNavigateur(): Promise<Dossier | null> {
  try {
    const storage = navigator.storage as { getDirectory?: () => Promise<unknown> } | undefined
    return storage?.getDirectory ? (await storage.getDirectory()) as Dossier : null
  } catch { return null }
}

async function marquer(d: Dossier) {
  const w = await (await d.getFileHandle(MARQUE, { create: true })).createWritable()
  await w.write(new Blob([String(Date.now())]))
  await w.close()
}

export async function purgerRestes(racine: Dossier | null, sauf: string | null = null, now = Date.now(), age = RESTES_MS): Promise<void> {
  if (!racine) return
  try {
    const base = await racine.getDirectoryHandle(RACINE)
    const noms: string[] = []
    for await (const [nom, e] of base.entries()) if (nom !== sauf && e.kind === 'directory') noms.push(nom)
    for (const nom of noms) {
      let vu = 0
      try { vu = (await (await (await base.getDirectoryHandle(nom)).getFileHandle(MARQUE)).getFile()).lastModified } catch { vu = 0 }
      if (now - vu > age) await base.removeEntry(nom, { recursive: true }).catch(() => {})
    }
  } catch { return }
}

export async function ouvrirDepot(cle: string | null | undefined, racine: Dossier | null): Promise<Depot> {
  if (!cle || !racine) return memoire()
  const nom = dossierDe(cle)
  try {
    const base = await racine.getDirectoryHandle(RACINE, { create: true })
    const dossier = await base.getDirectoryHandle(nom, { create: true })
    await marquer(dossier)
    const tailles = new Map<string, number>()
    for await (const [name, e] of dossier.entries()) {
      if (name === MARQUE || e.kind !== 'file') continue
      tailles.set(name, (await (await dossier.getFileHandle(name)).getFile()).size)
    }
    const secours = memoire()
    let size = 0
    for (const t of tailles.values()) size += t
    let closed = false
    return {
      has: (name) => tailles.has(name) || secours.has(name),
      get: async (name) => {
        if (secours.has(name)) return secours.get(name)
        if (!tailles.has(name)) return null
        try { return await (await dossier.getFileHandle(name)).getFile() } catch {
          size -= tailles.get(name) ?? 0
          tailles.delete(name)
          return null
        }
      },
      put: async (name, b) => {
        if (closed || tailles.has(name) || secours.has(name)) return
        try {
          const w = await (await dossier.getFileHandle(name, { create: true })).createWritable()
          await w.write(b)
          await w.close()
          if (closed) return
          tailles.set(name, b.size)
          size += b.size
        } catch {
          if (!closed) await secours.put(name, b)
        }
      },
      drop: (name) => {
        if (secours.has(name)) { secours.drop(name); return }
        if (!tailles.has(name)) return
        size -= tailles.get(name) ?? 0
        tailles.delete(name)
        void dossier.removeEntry(name).catch(() => {})
      },
      names: () => [...tailles.keys(), ...secours.names()],
      bytes: () => size + secours.bytes(),
      close: async (purge) => {
        closed = true
        await secours.close(true)
        if (!purge) return
        tailles.clear()
        size = 0
        await base.removeEntry(nom, { recursive: true }).catch(() => {})
      },
    }
  } catch { return memoire() }
}
