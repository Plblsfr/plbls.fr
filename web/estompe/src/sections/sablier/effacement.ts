import { confirmer } from '../../lib/confirm.svelte'
import { MOT_EFFACER } from './sablier'

const saisie = { label: `Écris ${MOT_EFFACER} pour confirmer`, exiger: MOT_EFFACER }

export const confirmerMonEffacement = () => confirmer({
  titre: 'Effacer toutes tes stats ?',
  message: 'C’est définitif. Seront effacés dans Sablier : tes stats et leur historique, tes trophées, tes récaps et tes réglages de partage. Ton historique Entracte et Sillon et tes messages ne sont pas touchés ; le journal des consultations est conservé. Pense à exporter avant si tu veux garder une trace.',
  action: 'Effacer définitivement',
  destructif: true,
  saisie,
})

export const confirmerEffacementDe = (nom: string) => confirmer({
  titre: `Effacer les données de ${nom} ?`,
  message: 'C’est définitif : stats et historique, trophées, récaps et réglages de partage. Rien d’autre n’est touché dans Entracte, Sillon ou la messagerie. L’effacement est noté au journal, avec ton nom. Si le compte existe encore, la collecte continue et ses stats repartent de zéro. Un import d’historique rejoué ensuite serait compté de nouveau pour cette personne.',
  action: 'Effacer définitivement',
  destructif: true,
  saisie,
})
