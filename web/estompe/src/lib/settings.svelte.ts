import type { IconName } from '../components/Icon.svelte'

export const SETTINGS_TABS = [
  { id: 'account', label: 'Compte' },
  { id: 'notifications', label: 'Notifications' },
  { id: 'voice', label: 'Voix et vidéo' },
  { id: 'appearance', label: 'Apparence' },
  { id: 'app', label: 'Application' },
  { id: 'debug', label: 'Débogage' },
  { id: 'reports', label: 'Signalements' },
] as const
export type SettingsTab = (typeof SETTINGS_TABS)[number]['id']

export const SETTINGS_ICONS: Record<SettingsTab, IconName> = {
  account: 'perso', notifications: 'cloche', voice: 'mic', appearance: 'soleil', app: 'telecharge', debug: 'bug', reports: 'drapeau',
}

export const SETTINGS_INDEX: Record<SettingsTab, readonly string[]> = {
  account: ['Avatar', 'Photo de profil par serveur', 'Se déconnecter', 'Profil', 'Session'],
  notifications: ['Messages privés et mentions', 'Notification de test', 'Ne pas déranger', 'Types de notifications', 'Messages privés', 'Mentions', 'Messages de salon',
    'Activité vocale', 'Contenu et son', 'Aperçu des notifications', 'Son à la réception', 'Par serveur et par salon', 'Ignorer @everyone et @here', 'Push'],
  voice: ['Micro', 'Sortie audio', 'Caméra', 'Volume du micro', 'Réduction de bruit', 'Activation du micro', 'Détection de voix', 'Push-to-talk', 'Tester mon micro',
    'Seuil de voix', 'Regarder automatiquement les streams', "Sons d'arrivée et de départ", 'Volume des sons', "Son de l'écran", 'Qualité de la caméra', 'Codec vidéo',
    'Arrière-plan de la caméra', 'Flou', 'Incrustation caméra', "Afficher ce que j'écoute", "Résolution du partage d'écran", 'Images par seconde'],
  appearance: ['Thème', 'Clair', 'Sombre', 'Densité des messages', 'Aperçu des liens', 'Transcrire automatiquement', 'Réduire les animations'],
  app: ['Mise à jour', 'Version installée', 'Télécharger', 'Windows', 'Android'],
  debug: ['Mode debug', 'Journal', 'Copier', 'Exporter', 'Vider'],
  reports: ['Signaler un bug', 'Mes signalements'],
}

const plain = (s: string) => s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase().trim()

export function searchSettings(query: string): { id: SettingsTab; label: string; hits: string[] }[] {
  const q = plain(query)
  return SETTINGS_TABS.flatMap((t) => {
    if (!q) return [{ id: t.id, label: t.label, hits: [] }]
    const hits = SETTINGS_INDEX[t.id].filter((s) => plain(s).includes(q))
    return plain(t.label).includes(q) || hits.length ? [{ id: t.id, label: t.label, hits }] : []
  })
}

/** `request` : onglet à ouvrir ; le Shell ouvre la fenêtre, qui le consomme. */
export const settingsNav = $state<{ request: SettingsTab | '' }>({ request: '' })
export const openSettings = (tab: SettingsTab) => { settingsNav.request = tab }
