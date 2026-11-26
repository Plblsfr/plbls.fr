import { expect, test, type Page } from '@playwright/test'
import { serveurTirelire, simulerTirelire } from './faux-tirelire'

const pdf = (name: string) => ({ name, mimeType: 'application/pdf', buffer: Buffer.from('%PDF-1.4 essai') })
const entree = (page: Page) => page.getByRole('link', { name: 'Tirelire, comptes' })
const onglets = (page: Page) => page.getByRole('navigation', { name: 'Sections de Tirelire' })
const liste = (page: Page) => page.getByRole('region', { name: 'Liste des opérations' })
const requetes = (srv: ReturnType<typeof serveurTirelire>) => srv.appels.filter((a) => a.chemin === '/operations').map((a) => a.requete)

test('sans tirelire:access, pas de section ; un lien direct ramène à l’accueil', async ({ page }) => {
  await simulerTirelire(page, { tirelire: false })
  await page.goto('/')
  await expect(page.getByRole('link', { name: 'Le Salon' })).toBeVisible()
  await expect(entree(page)).toHaveCount(0)
  await page.goto('/#/tirelire/operations')
  await expect(page).toHaveURL(/#\/$/)
  await expect(page.getByRole('heading', { name: 'Accueil', level: 1 })).toBeVisible()
})

test('Tirelire arrêté : l’app démarre sans la section', async ({ page }) => {
  const srv = serveurTirelire()
  srv.arrete = true
  await simulerTirelire(page, {}, srv)
  await page.goto('/')
  await expect(page.getByRole('link', { name: 'Le Salon' })).toBeVisible()
  await expect(entree(page)).toHaveCount(0)
})

test('premier lancement : aucun relevé, puis import de plusieurs PDF avec les trois issues', async ({ page }) => {
  const srv = await simulerTirelire(page)
  await page.goto('/')
  await entree(page).click()
  await expect(page).toHaveURL(/#\/tirelire$/)
  await expect(page.getByRole('heading', { name: 'Bienvenue', level: 1 })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Aucun relevé pour l’instant' })).toBeVisible()
  await page.getByRole('link', { name: 'Importer un relevé PDF' }).click()
  await expect(page).toHaveURL(/#\/tirelire\/import$/)
  await expect(page.getByText('Aucun relevé importé pour l’instant.')).toBeVisible()

  await page.getByLabel('Relevés PDF à importer').setInputFiles([pdf('releve-2026-11.pdf'), pdf('releve-2026-10.pdf')])
  await expect(page.getByRole('heading', { name: '2 fichiers traités' })).toBeVisible()
  await expect(page.getByRole('region', { name: 'Fichier importé : releve-2026-11.pdf' })).toContainText('Du 1er au 26 nov. 2026 · 41 opérations')
  await expect(page.getByRole('region', { name: 'Relevés importés' }).getByRole('listitem')).toHaveCount(2)

  await page.getByLabel('Relevés PDF à importer').setInputFiles([pdf('releve-2026-11.pdf'), pdf('releve-2026-08-refus.pdf'), pdf('releve-2026-09.pdf')])
  await expect(page.getByRole('heading', { name: '3 fichiers traités' })).toBeVisible()
  await expect(page.getByRole('list', { name: 'Bilan de l’import' }).getByRole('listitem')).toHaveText(['1 importé', '1 déjà importé', '1 refusé'])
  await expect(page.getByRole('region', { name: 'Fichier déjà importé : releve-2026-11.pdf' })).toContainText('aucune opération ajoutée')
  const refus = page.getByRole('region', { name: 'Fichier refusé : releve-2026-08-refus.pdf' })
  await expect(refus.getByRole('alert')).toContainText('Le contrôle a échoué')
  await expect(refus.getByRole('row').nth(1)).toHaveText(/Débits\s*2\s318,45\s€\s*2\s391,25\s€\s*−72,80\s€/)
  await expect(refus.getByRole('row').nth(2)).toHaveText(/Solde final\s*6\s226,80\s€\s*6\s154,00\s€\s*\+72,80\s€/)
  await expect(refus.getByRole('list', { name: 'Lignes non reconnues' }).getByRole('listitem')).toHaveText([/page 2\s*12\/08 CB\*0000 RETR\.DAB 60,0O/, /page 2\s*19\/08 VIR INST EXEMPLE 9,90/, /page 3\s*REF 44-A \/ 2,90/])
  expect(srv.releves.map((r) => r.number)).toEqual([11, 10, 9])

  await page.getByRole('link', { name: 'Voir les 39 nouvelles opérations' }).click()
  await expect(page).toHaveURL(/#\/tirelire\/operations$/)
  await expect(page.getByRole('group', { name: 'Résumé' })).toContainText('120')
})

test('suppression d’un relevé après confirmation', async ({ page }) => {
  const srv = await simulerTirelire(page, {}, serveurTirelire(true))
  await page.goto('/#/tirelire/import')
  const releves = page.getByRole('region', { name: 'Relevés importés' })
  await expect(releves.getByRole('listitem')).toHaveCount(2)
  await page.getByRole('button', { name: 'Supprimer le relevé n° 10 de 2026' }).click()
  const boite = page.getByRole('alertdialog')
  await expect(boite).toContainText('40 opérations seront supprimées avec lui.')
  await boite.getByRole('button', { name: 'Annuler' }).click()
  await expect(boite).toHaveCount(0)
  expect(srv.releves).toHaveLength(2)
  await page.getByRole('button', { name: 'Supprimer le relevé n° 10 de 2026' }).click()
  await page.getByRole('alertdialog').getByRole('button', { name: 'Supprimer' }).click()
  await expect(releves.getByRole('listitem')).toHaveCount(1)
  expect(srv.releves.map((r) => r.number)).toEqual([11])
  expect(srv.appels.some((a) => a.methode === 'DELETE' && a.chemin === '/statements/110')).toBe(true)
})

test('opérations : groupes par jour, suite, filtres par relevé et par période, détail', async ({ page }) => {
  const srv = await simulerTirelire(page, {}, serveurTirelire(true))
  await page.goto('/#/tirelire/operations')
  await expect(liste(page).getByRole('heading', { level: 3 }).first()).toHaveText(/Jeudi 26 novembre\s*−33,63\s€/)
  await expect(liste(page).getByRole('link')).toHaveCount(50)
  await expect(page.getByRole('group', { name: 'Résumé' })).toHaveText(/^\s*Opérations\s*104\s*$/)
  await page.getByRole('button', { name: 'Afficher la suite (54)' }).click()
  await expect(liste(page).getByRole('link')).toHaveCount(100)
  await page.getByRole('button', { name: 'Afficher la suite (4)' }).click()
  await expect(liste(page).getByRole('link')).toHaveCount(104)
  await expect(page.getByText('Toutes les opérations sont affichées')).toBeVisible()
  await expect(page.getByRole('group', { name: 'Résumé' })).toContainText('Crédits')

  await page.getByRole('combobox', { name: 'Relevé' }).selectOption({ label: 'Relevé n° 10 de 2026 · 26 oct. 2026' })
  await expect(page.getByRole('group', { name: 'Résumé' })).toContainText('40')
  await expect(liste(page).getByRole('heading', { level: 3 }).first()).toContainText('Lundi 26 octobre')
  await page.getByRole('button', { name: 'Afficher les filtres de période' }).click()
  await page.getByLabel('Du').fill('2026-10-20')
  await page.getByLabel('Au').fill('2026-10-22')
  await expect(page.getByRole('group', { name: 'Résumé' })).toHaveText(/Opérations\s*9\s*Débits/)
  expect(requetes(srv).at(-1)).toBe('?statement=110&from=2026-10-20&to=2026-10-22&limit=50&offset=0')
  await page.getByRole('button', { name: 'Réinitialiser' }).click()
  await expect(page.getByRole('group', { name: 'Résumé' })).toContainText('104')

  await liste(page).getByRole('link', { name: /PRLV STREAMO ABONNEMENT/ }).first().click()
  await expect(page).toHaveURL(/#\/tirelire\/operations\/\d+$/)
  await expect(page.getByRole('heading', { name: 'Opération', level: 1 })).toBeVisible()
  const releve = page.getByRole('region', { name: 'Libellé du relevé' })
  await expect(releve).toContainText('PRLV STREAMO ABONNEMENT')
  await expect(releve).toContainText('Détails (2 lignes)')
  await expect(releve).toContainText('VIREMENTS EMIS ET PRELEVEMENTS')
  await expect(releve).toContainText('Prélèvement')
  await expect(releve).toContainText('Date de valeur')
  await page.getByRole('link', { name: 'Retour aux opérations' }).click()
  await expect(page).toHaveURL(/#\/tirelire\/operations$/)
  await expect(liste(page)).toBeVisible()
})

test('onglets sans données : état honnête, sans chiffre', async ({ page }) => {
  await simulerTirelire(page, {}, serveurTirelire(true))
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('/#/tirelire')
  for (const [onglet, titre] of [['Mois', 'Vue du mois'], ['Évolution', 'Évolution'], ['Habitudes', 'Habitudes'], ['Accueil', 'Accueil']]) {
    await onglets(page).getByRole('link', { name: onglet }).click()
    await expect(page.getByRole('heading', { name: titre, level: 1 })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Disponible quand les catégories seront en place' })).toBeVisible()
    await expect(onglets(page).getByRole('link', { name: onglet })).toHaveAttribute('aria-current', 'page')
    await expect(page.locator('.tir')).not.toContainText('€')
  }
})

test('téléphone (390 px) : barre d’onglets, cibles de 44 px, rien ne déborde ; bureau : menu latéral', async ({ page }) => {
  await simulerTirelire(page, {}, serveurTirelire(true))
  await page.setViewportSize({ width: 390, height: 844 })
  for (const chemin of ['/#/tirelire', '/#/tirelire/operations', '/#/tirelire/import']) {
    await page.goto(chemin)
    await expect(page.locator('.tir-corps > *').first()).toBeVisible()
    await expect(page.locator('.tir').getByRole('status')).toHaveCount(0)
    const mesures = await page.evaluate(() => ({
      deborde: [...document.querySelectorAll('.tir *')].filter((e) => { const r = e.getBoundingClientRect(); return r.width > 0 && (r.right > innerWidth + 0.5 || r.left < -0.5) }).map((e) => e.className),
      petites: [...document.querySelectorAll<HTMLElement>('.tir a, .tir button, .tir select, .tir input:not(.v-sr)')].filter((e) => { const r = e.getBoundingClientRect(); return r.width > 0 && (r.height < 43.5 || r.width < 43.5) }).map((e) => `${e.tagName} ${e.className} ${e.textContent?.trim().slice(0, 20)}`),
    }))
    expect(mesures, chemin).toEqual({ deborde: [], petites: [] })
  }
  await page.goto('/#/tirelire/operations')
  await expect(onglets(page)).toBeVisible()
  await page.setViewportSize({ width: 1280, height: 900 })
  await expect(onglets(page)).toBeHidden()
  const menu = page.getByRole('navigation', { name: 'Tirelire', exact: true })
  await expect(menu.getByRole('link', { name: 'Opérations' })).toHaveAttribute('aria-current', 'page')
  await menu.getByRole('link', { name: 'Import' }).click()
  await expect(page).toHaveURL(/#\/tirelire\/import$/)
})

test('Tirelire tombe en cours de route : message et relance, le reste de l’app continue', async ({ page }) => {
  const srv = await simulerTirelire(page, {}, serveurTirelire(true))
  await page.goto('/')
  await expect(entree(page)).toBeVisible()
  srv.arrete = true
  await entree(page).click()
  await expect(page.getByRole('alert')).toContainText('Tirelire est injoignable.')
  srv.arrete = false
  await page.getByRole('button', { name: 'Réessayer' }).click()
  await expect(page.getByRole('heading', { name: 'Disponible quand les catégories seront en place' })).toBeVisible()
})

test('accessibilité : aucune violation axe sur les écrans de la section', async ({ page }) => {
  const { default: AxeBuilder } = await import('@axe-core/playwright')
  await simulerTirelire(page, {}, serveurTirelire(true))
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('/#/tirelire/import')
  await page.getByLabel('Relevés PDF à importer').setInputFiles([pdf('releve-2026-11.pdf'), pdf('releve-2026-08-refus.pdf'), pdf('releve-2026-09.pdf')])
  await expect(page.getByRole('heading', { name: '3 fichiers traités' })).toBeVisible()
  const verifier = async (ou: string) => {
    await page.waitForFunction(() => document.getAnimations().every((a) => a.playState !== 'running'))
    const res = await new AxeBuilder({ page }).include('#main').withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
    expect(res.violations.map((v) => `${ou} · ${v.id} : ${v.nodes.map((n) => `${n.target.join(' ')} ${n.failureSummary}`).join(', ')}`)).toEqual([])
  }
  await verifier('import')
  for (const [chemin, attente] of [['/#/tirelire', 'Pas encore disponible'], ['/#/tirelire/operations', 'Liste des opérations']]) {
    await page.goto(chemin)
    await expect(page.getByRole('region', { name: attente })).toBeVisible()
    await verifier(chemin)
  }
  await page.getByRole('button', { name: 'Afficher les filtres de période' }).click()
  await verifier('filtres')
  await liste(page).getByRole('link').first().click()
  await expect(page.getByRole('region', { name: 'Libellé du relevé' })).toBeVisible()
  await verifier('détail')
})
