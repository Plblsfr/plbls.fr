/** Blocs du frontend : mêmes noms et dépendances que les blocs du backend (`block_list()` dans backend/estompe-api/src/main.rs). */
export interface BlockDef { name: string; needs?: string[] }

export const BLOCKS: BlockDef[] = [
  { name: 'gifs' },
  { name: 'musique' },
  { name: 'vocal' },
  { name: 'regarder-ensemble', needs: ['vocal'] },
  { name: 'draw2gether', needs: ['vocal'] },
  { name: 'transcription', needs: ['vocal'] },
  { name: 'videos' },
  { name: 'apercu-liens' },
  { name: 'notifications' },
  { name: 'emojis' },
  { name: 'avatars' },
  { name: 'badges' },
  { name: 'signalements' },
  { name: 'activite-statut' },
  { name: 'films', needs: ['vocal', 'regarder-ensemble'] },
]

/** Un bloc est proposé s'il est annoncé par le serveur (`blocks` de `GET /me`) avec toutes ses dépendances ; sans liste (ancien serveur), tous le sont. */
export function blockOn(active: string[] | undefined, name: string): boolean {
  if (!active) return true
  const def = BLOCKS.find((b) => b.name === name)
  return !!def && active.includes(name) && (def.needs ?? []).every((n) => blockOn(active, n))
}
