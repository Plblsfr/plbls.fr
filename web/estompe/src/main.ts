import './platform-init'
import '@fontsource-variable/bricolage-grotesque'
import '@fontsource-variable/instrument-sans'
import '@fontsource-variable/jetbrains-mono'
import './vendor/plbls-ui/plbls-ui.min.css'
import './vendor/plbls-ui/plbls-ui.esm.min.js'
import './styles/v3-jetons.css'
import './styles/v3-kit.css'
import './styles/acces.css'
import './app.css'
import './styles/gabarit.css'
import './styles/vocal.css'
import './styles/parametres.css'
import './styles/messagerie.css'
import './styles/permissions.css'
import { mount } from 'svelte'
import App from './App.svelte'
import { startActivity } from './blocks/activite-statut/activity.svelte'
import { debug } from './lib/debuglog.svelte'
import { platform } from './lib/platform'
import { startViewport } from './lib/viewport'
import { updater } from './lib/updater.svelte'

// Fenêtres annexes de la plateforme (overlays) : page légère, sans session ni mises à jour.
const aux = platform.windows[location.hash]
if (!aux) {
  startViewport()
  debug.start() // mode debug : rétabli tel que laissé
  platform.auth?.setup() // apps natives : Bearer au lieu du cookie de session
  updater.start() // version déclarée à l'API, mises à jour
  platform.start() // ce que la plateforme lance au démarrage
  startActivity() // jeu en cours (si la plateforme sait le détecter) et morceau écouté
}

export default mount(aux ?? App, { target: document.getElementById('app')! })
