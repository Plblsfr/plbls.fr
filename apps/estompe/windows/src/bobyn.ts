// Bobyn (alpha) : replay instantané des dernières secondes de jeu, app Windows uniquement. Le moteur est en Rust (windows/src-tauri/bobyn).
import { prefs } from '../../../../web/estompe/src/lib/prefs.svelte'

export interface BobynAudio { source: string; gain: number; device?: string | null; ptt?: string | null }
export interface BobynConfig {
  enabled: boolean; seconds: number; fps: number; mbps: number; height: number | null; codec: string
  mode: 'game' | 'window' | 'screen'; window: string; screen: number; always_on: boolean; cursor: boolean | null; dir: string | null
  hotkey: string; hotkey_last: string | null; hotkey_marker: string | null; last_seconds: number; games: string[]; audio: BobynAudio[]; beep: boolean; notify: boolean
  marker_beep: boolean; marker_autosave: boolean
}
export interface BobynStatus { state: 'idle' | 'recording' | 'paused' | 'error'; text: string }
export interface BobynMachine {
  windows: { exe: string; title: string }[]; monitors: { name: string; width: number; height: number; primary: boolean }[]
  audio_apps: string[]; mics: { id: string; name: string }[]; codecs: string[]
}
export interface BobynState { config: BobynConfig; error: string | null; status: BobynStatus | null; machine: BobynMachine }

/** Onglet Bobyn : app Windows et alpha débloquée (7 clics sur la version dans « Application »). */
export const bobynVisible = () => prefs.alpha

const core = () => import('@tauri-apps/api/core')
export const bobynState = async () => (await core()).invoke<BobynState>('bobyn_state')
export const bobynApply = async (config: BobynConfig) => (await core()).invoke('bobyn_apply', { config })
export const bobynSave = async (last: boolean) => (await core()).invoke('bobyn_save', { last })

/** Débloque ou verrouille l'alpha ; verrouillée, le service est arrêté (plus aucune capture sans onglet pour la couper). */
export async function setAlpha(on: boolean) {
  prefs.alpha = on
  prefs.save()
  if (!on) {
    const s = await bobynState().catch(() => null)
    if (s?.config.enabled) await bobynApply({ ...s.config, enabled: false }).catch(() => {})
  }
}

export interface AudioRow { source: string; label: string; on: boolean; gain: number }

/** Sources audio hors micro : le jeu, le système, les applications qui jouent du son, et celles déjà choisies mais absentes. */
export function audioRows(saved: BobynAudio[], apps: string[]): AudioRow[] {
  const chosen = saved.filter((a) => a.source !== 'mic')
  const known = [
    { source: 'game', label: 'Le jeu détecté' },
    { source: 'voice', label: 'Le vocal Estompe (voix des autres)' },
    { source: 'system', label: 'Tout le son du système' },
    { source: 'system-novoice', label: "Le système sans la voix du vocal d'Estompe" },
    ...apps.map((a) => ({ source: `app:${a}`, label: a })),
  ]
  for (const a of chosen) if (!known.some((k) => k.source === a.source)) known.push({ source: a.source, label: a.source.replace(/^app:/, '') })
  return known.map((k) => {
    const s = chosen.find((a) => a.source === k.source)
    return { ...k, on: !!s, gain: s?.gain ?? 1 }
  })
}

/** Micro WASAPI qui correspond au micro choisi dans les Paramètres d'Estompe (identifiant WebRTC) : on les rapproche par leur nom, au préfixe « Par défaut - » près. */
export function micFromPrefs(label: string, mics: { id: string; name: string }[]): string | null {
  const l = label.replace(/^(Par défaut|Default|Communications) - /i, '').trim().toLowerCase()
  return (l && mics.find((m) => m.name.toLowerCase() === l)?.id) || null
}

/** Inverse d'`audioRows` ; `mic` : '' = désactivé, 'default' = périphérique par défaut, sinon son identifiant. */
export function audioConfig(rows: AudioRow[], mic: string, ptt: string): BobynAudio[] {
  const out: BobynAudio[] = rows.filter((r) => r.on).map((r) => ({ source: r.source, gain: r.gain }))
  if (mic) out.push({ source: 'mic', gain: 1, device: mic === 'default' ? null : mic, ptt: ptt.trim() || null })
  return out
}

/** Raccourci au format de Bobyn (« Ctrl+Alt+F10 ») d'après un évènement clavier ; '' pour un modificateur seul ou une touche non gérée (lettres, chiffres, F1-F24). */
export function bobynHotkey(e: Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): string {
  const f = /^F(\d{1,2})$/.exec(e.code)
  const key = /^Key([A-Z])$/.exec(e.code)?.[1] ?? /^Digit(\d)$/.exec(e.code)?.[1] ?? (f && +f[1] >= 1 && +f[1] <= 24 ? e.code : '')
  return key ? [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win', key].filter(Boolean).join('+') : ''
}
