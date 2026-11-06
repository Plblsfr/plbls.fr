// Signalement de bugs (EST-105) : catégories, informations techniques et brouillon du formulaire.
import { debug, formatEntry } from '../../lib/debuglog.svelte'
import { app } from '../../lib/store.svelte'
import { router } from '../../lib/route.svelte'
import { voice } from '../vocal/voice.svelte'
import { platform } from '../../lib/platform'
import type { ReportStatus } from '../../lib/types'

export const REPORT_CATEGORIES = [
  { id: '', label: 'Non précisée' }, { id: 'messages', label: 'Messages' }, { id: 'vocal', label: 'Vocal' }, { id: 'ecran', label: "Partage d'écran" },
  { id: 'notifications', label: 'Notifications' }, { id: 'bobyn', label: 'Bobyn' }, { id: 'autre', label: 'Autre' },
]
export const REPORT_PLATFORMS = [{ id: 'web', label: 'Web' }, { id: 'pwa', label: 'PWA' }, { id: 'desktop', label: 'Application PC' }, { id: 'android', label: 'Android' }]
export const REPORT_STATUS: Record<ReportStatus, string> = { new: 'Nouveau', in_progress: 'En cours', resolved: 'Résolu', rejected: 'Rejeté', duplicate: 'Doublon' }
export const label = (list: { id: string; label: string }[], id: string) => list.find((x) => x.id === id)?.label ?? (id || '—')

export const MAX_SHOTS = 5
const MAX_LOG_LINES = 300
const DRAFT_KEY = 'estompe.report-draft'

/** Plateforme au vocabulaire du serveur (l'app Windows s'annonce « windows » au temps réel). */
export const reportPlatform = () => { const p = platform.id(); return p === 'windows' ? 'desktop' : p }

/** Dernières lignes du journal debug, déjà expurgées à l'écriture (voir `redact`). */
export const recentLogs = () => debug.entries.slice(-MAX_LOG_LINES).map(formatEntry).join('\n')

/** Informations ajoutées au signalement ; chaque ligne est décochable avant l'envoi. */
export function techInfo(): Record<string, string> {
  const r = router.current
  const server = r.name === 'channel' ? app.details[r.serverId] : undefined
  const channel = server?.channels.find((c) => c.id === (r.name === 'channel' ? r.channelId : ''))
  const where = r.name === 'channel' ? `serveur « ${server?.server.name ?? '?'} », salon « ${channel?.name ?? '?'} »` : r.name === 'dm' ? 'message privé' : r.name
  const call = voice.channelId ? app.channel(voice.channelId) : null
  return {
    Plateforme: reportPlatform(),
    Version: __APP_VERSION__,
    Système: navigator.platform || 'inconnu',
    Navigateur: navigator.userAgent,
    Écran: where,
    Vocal: voice.state === 'idle' || !call ? 'inactif' : `${voice.state} dans « ${call.channel.name} »`,
  }
}

export interface Draft { title: string; description: string; category: string }
export function loadDraft(): Draft {
  try {
    const d = JSON.parse(localStorage.getItem(DRAFT_KEY) ?? '')
    if (typeof d.title === 'string' && typeof d.description === 'string') return { title: d.title, description: d.description, category: String(d.category ?? '') }
  } catch { /* pas de brouillon */ }
  return { title: '', description: '', category: '' }
}
export function saveDraft(d: Draft) {
  try { if (d.title || d.description) localStorage.setItem(DRAFT_KEY, JSON.stringify(d)); else localStorage.removeItem(DRAFT_KEY) } catch { /* stockage indisponible */ }
}

/** Texte à reprendre à la main ailleurs (bouton « Copier » de l'administration). */
export function formatReport(r: { title: string; author?: string; created_at: string; category: string; platform: string; status: string; description?: string; tech?: Record<string, string>; logs?: string }): string {
  const tech = Object.entries(r.tech ?? {}).map(([k, v]) => `- ${k} : ${v}`).join('\n')
  return [`# ${r.title}`, `Signalé par ${r.author ?? '?'} le ${new Date(r.created_at).toLocaleString('fr-FR')} · ${label(REPORT_CATEGORIES, r.category)} · ${label(REPORT_PLATFORMS, r.platform)} · ${REPORT_STATUS[r.status as ReportStatus] ?? r.status}`,
    r.description ?? '', tech && `## Informations techniques\n${tech}`, r.logs && `## Logs\n${r.logs}`].filter(Boolean).join('\n\n')
}
