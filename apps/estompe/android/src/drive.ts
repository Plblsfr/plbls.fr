/** Mode conduite de l'app Android : réglages gardés côté Kotlin (DriveService), échangés en JSON par le pont natif. */
export type DriveSettings = { engine: string; rate: number; readContent: boolean; confirm: boolean; autoBluetooth: boolean; bluetooth: string }
export type Choice = { id: string; label: string }

export const DRIVE_DEFAULTS: DriveSettings = { engine: '', rate: 1, readContent: true, confirm: true, autoBluetooth: false, bluetooth: '' }

/** Lit le JSON du pont ; une valeur absente ou mal typée retombe sur le défaut, une vitesse hors plage est ramenée dans [0,5 ; 2]. */
export function parseDrive(json: string): DriveSettings {
  let j: Record<string, unknown> = {}
  try { j = JSON.parse(json) ?? {} } catch { /* pont absent ou réponse invalide : défauts */ }
  const d = DRIVE_DEFAULTS
  const str = (k: keyof DriveSettings, def: string) => (typeof j[k] === 'string' ? (j[k] as string) : def)
  const bool = (k: keyof DriveSettings, def: boolean) => (typeof j[k] === 'boolean' ? (j[k] as boolean) : def)
  const rate = typeof j.rate === 'number' && Number.isFinite(j.rate) ? j.rate : d.rate
  return {
    engine: str('engine', d.engine), rate: Math.min(2, Math.max(0.5, rate)), readContent: bool('readContent', d.readContent),
    confirm: bool('confirm', d.confirm), autoBluetooth: bool('autoBluetooth', d.autoBluetooth), bluetooth: str('bluetooth', d.bluetooth),
  }
}

/** Liste « id / libellé » renvoyée par le pont (moteurs de voix, appareils Bluetooth) ; vide si illisible. */
export function parseChoices(json: string): Choice[] {
  try {
    const a: unknown = JSON.parse(json)
    return Array.isArray(a) ? a.filter((c): c is Choice => typeof c?.id === 'string' && typeof c?.label === 'string') : []
  } catch {
    return []
  }
}
