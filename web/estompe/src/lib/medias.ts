export type SourceMedia = 'film' | 'vocal' | 'sillon'

type Ecouteur = (source: SourceMedia) => void

const ecouteurs = new Set<Ecouteur>()

export const medias = {
  demarre(source: SourceMedia) { for (const f of [...ecouteurs]) f(source) },
  ecouter(f: Ecouteur) { ecouteurs.add(f); return () => { ecouteurs.delete(f) } },
}
