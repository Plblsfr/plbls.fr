import { fireEvent, render, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../lib/api'
import { CATEGORIES, moisStats, operation, releve } from './essais'
import { habitudesStats, RECURRENTS_ESSAI, recurrentsCourts, recurrentsStats, totaliser } from './essais-habitudes'
import TirelireAbonnements from './TirelireAbonnements.svelte'
import TirelireAccueil from './TirelireAccueil.svelte'
import TirelireHabitudes from './TirelireHabitudes.svelte'
import TirelireOperation from './TirelireOperation.svelte'
import { tirelireApi } from './tirelire'
import { tirelire } from './tirelire.svelte'

const texte = (el: Element | null) => el?.textContent?.replace(/\s+/g, ' ').trim() ?? ''

beforeEach(() => {
  tirelire.reinitialiser()
  localStorage.clear()
  vi.spyOn(tirelireApi, 'categories').mockResolvedValue(CATEGORIES)
  vi.spyOn(tirelireApi, 'compteur').mockResolvedValue({ operations: 0, merchants: 0, debits: 0, credits: 0 })
  vi.spyOn(tirelireApi, 'releves').mockResolvedValue([releve()])
})
afterEach(() => { vi.restoreAllMocks(); tirelire.reinitialiser() })

describe('TirelireHabitudes', () => {
  it('affiche commerçants, petits achats, jours, semaines et opérations inhabituelles', async () => {
    await tirelire.ouvrir()
    tirelire.changerFenetreHabitudes('1')
    const vue = render(TirelireHabitudes, { habitudes: habitudesStats(), recurrents: recurrentsStats() })
    const top = within(vue.getByRole('region', { name: 'Top commerçants' }))
    expect(top.getAllByRole('listitem').map((li) => texte(li.querySelector('.tir-h3')))).toEqual(['1.Marché Colline', '2.Mode Atelier Nord', '3.Livréo', '4.Épicerie du Canal', '5.Boulangerie Mie Dorée'])
    expect(top.getAllByRole('link')[0]).toHaveAttribute('href', '#/tirelire/operations?from=2026-11-01&to=2026-11-26&merchant=41&direction=debit&date=purchase')
    expect(texte(top.getAllByRole('listitem')[0])).toContain('9 passages · panier 29,82 € · Courses')
    await fireEvent.click(top.getByRole('button', { name: 'Par nombre' }))
    expect(texte(top.getAllByRole('listitem')[0])).toMatch(/Boulangerie Mie Dorée ?14 passages/)
    expect(texte(top.getAllByRole('listitem')[0])).toContain('à classer')
    const petits = texte(vue.getByRole('region', { name: 'Petits achats répétés' }))
    expect(petits).toMatch(/Achats ?37/)
    expect(petits).toContain('≈ 2 641 €')
    expect(petits).toContain('d’après les 26 jours que tes relevés couvrent')
    const jours = within(vue.getByRole('region', { name: 'Par jour de la semaine' }))
    expect(jours.getByRole('img').getAttribute('aria-label')?.replace(/\s+/g, ' ')).toContain('samedi 292,00 €')
    expect(texte(jours.getByRole('row', { name: /vendredi/ }))).toMatch(/^vendredi ?246,00 € ?11 ?82,00 €$/)
    expect(texte(vue.getByRole('region', { name: 'Par jour de la semaine' }))).toContain('Seuls les paiements par carte comptent ici')
    expect(texte(vue.getByRole('region', { name: 'Par semaine du mois' }))).toContain('S3 · 15–21')
    const rares = within(vue.getByRole('region', { name: 'Opérations inhabituelles' }))
    expect(rares.getAllByRole('link').map((a) => a.getAttribute('href'))).toEqual(['#/tirelire/operations/31', '#/tirelire/operations/32'])
    expect(texte(rares.getAllByRole('listitem')[0])).toContain('2,7 × ton panier habituel chez Livréo (15,32 €)')
    expect(texte(vue.getByRole('link', { name: /Abonnements et prélèvements/ }))).toContain('2 alertes')
  })

  it('dit ce qui manque : pas d’estimation annuelle, rien de répété, aucun paiement par carte', async () => {
    const h = habitudesStats()
    const vide = { amount: 0, count: 0 }
    const vue = render(TirelireHabitudes, { habitudes: habitudesStats({ previous: null, merchants: [], merchants_total: 0, small: { ...h.small, yearly: null, covered_days: 12, merchants: h.small.merchants.map((m) => ({ ...m, yearly: null })) }, weekdays: { ...vide, buckets: h.weekdays.buckets.map((t) => ({ ...t, ...vide })) }, unusual: { total: 0, operations: [] } }), recurrents: null })
    expect(texte(vue.getByRole('region', { name: 'Top commerçants' }))).toContain('Aucune dépense variable chez un commerçant')
    expect(texte(vue.getByRole('region', { name: 'Petits achats répétés' }))).toContain('Pas d’estimation sur un an : tes relevés ne couvrent que 12 jours de cette fenêtre, il en faut au moins 28.')
    expect(texte(vue.getByRole('region', { name: 'Par jour de la semaine' }))).toContain('Aucun paiement par carte sur cette fenêtre')
    expect(texte(vue.getByRole('region', { name: 'Opérations inhabituelles' }))).toContain('Rien d’inhabituel')
    vue.unmount()
    const sans = render(TirelireHabitudes, { habitudes: habitudesStats({ small: { ...h.small, count: 0, amount: 0, yearly: null, merchants: [], merchants_total: 0 } }), recurrents: recurrentsCourts() })
    expect(texte(sans.getByRole('region', { name: 'Petits achats répétés' }))).toContain('Aucun petit achat répété')
    expect(texte(sans.getByRole('link', { name: /Abonnements et prélèvements/ }))).toContain('aucun récurrent détecté pour l’instant')
  })
})

describe('TirelireAbonnements', () => {
  it('totaux, alertes, liste filtrée, arrêtés et écartés', async () => {
    await tirelire.ouvrir()
    const vue = render(TirelireAbonnements, { recurrents: recurrentsStats() })
    expect(texte(vue.getByRole('region', { name: 'Coût mensuel' }))).toMatch(/^Par mois ?958,67 € ?6 paiements récurrents$/)
    expect(texte(vue.getByRole('region', { name: 'Coût annuel' }))).toContain('11 504,04 €')
    const alertes = within(vue.getByRole('region', { name: 'Alertes' }))
    expect(alertes.getByRole('heading', { name: '2 alertes' })).toBeInTheDocument()
    expect(texte(alertes.getByRole('article', { name: 'Montant changé : Streamo' }))).toMatch(/ancien montant : ?11,99 € ?nouveau montant : ?13,99 €/)
    expect(alertes.getByRole('link', { name: 'Voir l’opération' })).toHaveAttribute('href', '#/tirelire/operations/64')
    const tous = within(vue.getByRole('region', { name: 'Tous les récurrents' }))
    expect(tous.getAllByRole('listitem').map((li) => texte(li.querySelector('.tir-h3')))).toEqual(['Régie Belvédère', 'Panier Vert', 'Voltéa Énergie', 'Salle Tonus', 'Streamo', 'Arcadia+'])
    expect(texte(tous.getAllByRole('listitem')[0])).toMatch(/mensuel, vers le 5 · 780,00 € ?depuis juin 2026 · prochaine vers le 5 déc\. 2026 ?Logement ?9 360,00 € ?par an/)
    await fireEvent.click(tous.getByRole('button', { name: 'Charges · 2' }))
    expect(tous.getAllByRole('listitem')).toHaveLength(2)
    expect(texte(vue.getByRole('region', { name: 'Tous les récurrents' }))).toMatch(/Total ?10 113,48 € par an ?842,79 € par mois/)
    expect(texte(vue.getByRole('region', { name: 'Arrêtés' }))).toContain('Presse Le Fil')
    expect(texte(vue.getByRole('region', { name: 'Arrêtés' }))).toContain('dernière échéance le 15 sept. 2026')
    expect(within(vue.getByRole('region', { name: 'Écartés' })).getByRole('button', { name: 'Rétablir Bistrot Lune' })).toBeEnabled()
    expect(texte(vue.getByRole('region', { name: 'Méthode' }))).toContain('Il faut 13 mois pour un abonnement annuel.')
  })

  it('marque une alerte comme vue, écarte un faux abonnement et affiche une erreur', async () => {
    const vues = totaliser(RECURRENTS_ESSAI.map((x) => (x.id === '54-monthly' ? { ...x, alerts: x.alerts.map((a) => ({ ...a, seen: true })) } : x)))
    const modifier = vi.spyOn(tirelireApi, 'modifierRecurrent').mockResolvedValueOnce(vues).mockRejectedValueOnce(new ApiError(503, 'unavailable', 'indisponible'))
    tirelire.recurrents = recurrentsStats()
    const vue = render(TirelireAbonnements, { recurrents: recurrentsStats() })
    await fireEvent.click(within(vue.getByRole('article', { name: 'Montant changé : Streamo' })).getByRole('button', { name: 'C’est noté' }))
    await waitFor(() => expect(texte(vue.getByRole('status'))).toBe('Alerte de « Streamo » marquée comme vue.'))
    expect(modifier).toHaveBeenCalledWith('54-monthly', { seen: true })
    expect(tirelire.recurrents?.alerts_unseen).toBe(1)
    await fireEvent.click(vue.getByRole('button', { name: 'Écarter Régie Belvédère : ce n’est pas un abonnement' }))
    await waitFor(() => expect(texte(vue.getByRole('alert'))).toBe('Tirelire est indisponible pour le moment.'))
    expect(modifier).toHaveBeenLastCalledWith('51-monthly', { dismissed: true })
    expect(tirelire.recurrents?.alerts_unseen).toBe(1)
  })

  it('historique trop court : dit ce qu’il faudrait', () => {
    const vue = render(TirelireAbonnements, { recurrents: recurrentsCourts() })
    const vide = vue.getByRole('region', { name: 'Aucun récurrent détecté' })
    expect(within(vide).getByRole('heading', { name: 'Historique trop court pour repérer une cadence' })).toBeInTheDocument()
    expect(texte(vide)).toContain('1 relevé importé · 26 jours couverts')
    expect(texte(vide)).toContain('avec un seul relevé, rien ne peut encore se confirmer')
    expect(within(vide).getByRole('link', { name: 'Importer d’autres relevés' })).toHaveAttribute('href', '#/tirelire/import')
    vue.unmount()
    const sans = render(TirelireAbonnements, { recurrents: totaliser(RECURRENTS_ESSAI.map((x) => ({ ...x, alerts: [] }))) })
    expect(texte(sans.getByRole('region', { name: 'Alertes' }))).toContain('Aucune alerte')
  })
})

describe('accueil et détail', () => {
  it('carte d’accès aux abonnements avec le nombre d’alertes non vues', () => {
    const vue = render(TirelireAccueil, { mois: moisStats(), recurrents: recurrentsStats() })
    const carte = vue.getByRole('link', { name: 'Abonnements et prélèvements : 2 alertes non vues' })
    expect(carte).toHaveAttribute('href', '#/tirelire/habitudes/abonnements')
    expect(texte(carte)).toMatch(/958,67 € par mois · 6 récurrents ?2 alertes/)
    vue.unmount()
    const calme = render(TirelireAccueil, { mois: moisStats(), recurrents: totaliser(RECURRENTS_ESSAI.map((x) => ({ ...x, alerts: [] }))) })
    expect(texte(calme.getByRole('link', { name: 'Abonnements et prélèvements' }))).not.toContain('alerte')
    calme.unmount()
    expect(render(TirelireAccueil, { mois: moisStats() }).queryByRole('link', { name: /Abonnements et prélèvements/ })).toBeNull()
  })

  it('étiquette « inhabituelle » et sa raison dans le détail d’une opération', async () => {
    vi.spyOn(tirelireApi, 'operation').mockResolvedValue(operation({ id: 31, merchant_name: 'Livréo', amount: 4180 }))
    const raison = vi.spyOn(tirelireApi, 'inhabituelle').mockResolvedValue(habitudesStats().unusual.operations[0].reason)
    const vue = render(TirelireOperation, { id: 31 })
    const note = await vue.findByRole('note')
    expect(texte(note)).toMatch(/^Inhabituelle ?2,7 × ton panier habituel chez Livréo \(15,32 €\) ?Comparée à la médiane de 9 autres achats chez ce commerçant\.$/)
    expect(raison).toHaveBeenCalledWith(31)
    vue.unmount()
    tirelire.reinitialiser()
    raison.mockResolvedValue(null)
    const normale = render(TirelireOperation, { id: 31 })
    await normale.findByRole('region', { name: 'Résumé de l’opération' })
    await waitFor(() => expect(raison).toHaveBeenCalledTimes(2))
    expect(normale.queryByRole('note')).toBeNull()
  })
})
