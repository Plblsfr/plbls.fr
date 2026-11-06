import { app } from '../../lib/store.svelte'
import { prefs } from '../../lib/prefs.svelte'

/** La transcription des messages vocaux est-elle proposée à cette personne ? (« alpha » : seulement avec les fonctions alpha débloquées) */
export const transcriptionOn = () => app.transcription === 'on' || (app.transcription === 'alpha' && prefs.alpha)
