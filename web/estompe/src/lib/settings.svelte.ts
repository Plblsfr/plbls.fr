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

/** `request` : onglet à ouvrir ; le Shell ouvre la fenêtre, qui le consomme. */
export const settingsNav = $state<{ request: SettingsTab | '' }>({ request: '' })
export const openSettings = (tab: SettingsTab) => { settingsNav.request = tab }
