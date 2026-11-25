import { expect, test, type Page, type Route } from '@playwright/test'

const ORIGINE = 'http://localhost:4178'
const ESTOMPE = 'http://estompe.test'
const ENTRACTE = 'http://entracte.test'
const MODULE_VOCAL = '/src/blocks/vocal/voice.svelte.ts'

const cors = { 'access-control-allow-origin': ORIGINE, 'access-control-allow-credentials': 'true', 'access-control-allow-headers': '*', 'access-control-allow-methods': '*' }
const json = (route: Route, body: unknown) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body), headers: cors })
const serveur = { id: 's1', name: 'Amis', owner_id: 'me', icon_version: 0 }
const personnes = [{ user_id: 'me', pseudo: 'moi' }, { user_id: 'u2', pseudo: 'camille' }]
const pistes = {
  audio: [{ n: 0, label: 'English (EAC3, 5.1)', language: 'eng' }, { n: 1, label: 'VFF (AC3, 5.1)', language: 'fre' }],
  subtitles: [{ n: 0, label: 'English', language: 'eng', forced: false }, { n: 1, label: 'Français', language: 'fre', forced: false }],
}

const FILM_VP9 = Buffer.from('GkXfo59ChoEBQveBAULygQRC84EIQoKEd2VibUKHgQJChYECGFOAZwEAAAAAAAOeEU2bdLpNu4tTq4QVSalmU6yBoU27i1OrhBZUrmtTrIHYTbuMU6uEElTDZ1OsggEeTbuMU6uEHFO7a1OsggOI7AEAAAAAAABZAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAVSalmsirXsYMPQkBNgI1MYXZmNjAuMTYuMTAwV0GNTGF2ZjYwLjE2LjEwMESJiEDTiAAAAAAAFlSua8GuAQAAAAAAADjXgQFzxYhjnbHvSrt195yBACK1nIN1bmSIgQCGhVZfVlA5g4EBI+ODhDuaygDgibCBQLqBJJqBAhJUw2dAgHNzoGPAgGfImkWjh0VOQ09ERVJEh41MYXZmNjAuMTYuMTAwc3PaY8CLY8WIY52x70q7dfdnyKVFo4dFTkNPREVSRIeYTGF2YzYwLjMxLjEwMiBsaWJ2cHgtdnA5Z8ihRaOIRFVSQVRJT05Eh5MwMDowMDoyMC4wMDAwMDAwMDAAH0O2dUCb54EAo62BAACAgkmDQgAD8AI2ADgkHBhCAAAwYAAAWg7//+OkwP////41gv///92xIACjk4ED6ACGAECSHABeAAADYAAAVHCjk4EH0ACGAECSHABeAAADYAAAVHCjk4ELuACGAECSHABeAAADYAAAVHCjk4EPoACGAECSHABeAAADYAAAVHCjk4ETiACGAECSHABeAAADYAAAVHAfQ7Z1QILnghdwo5OBAAAAhgBAkhwAXgAAA2AAAFRwo5OBA+gAhgBAkhwAXgAAA2AAAFRwo5OBB9AAhgBAkhwAXgAAA2AAAFRwo5OBC7gAhgBAkhwAXgAAA2AAAFRwo5OBD6AAhgDAkpwAUOAAA2AAAFRwo5OBE4gAhgBAkhwAXgAAA2AAAFRwH0O2dUCC54Iu4KOTgQAAAIYAQJIcAF4AAANgAABUcKOTgQPoAIYAQJIcAF4AAANgAABUcKOTgQfQAIYAQJIcAF4AAANgAABUcKOTgQu4AIYAQJIcAF4AAANgAABUcKOTgQ+gAIYAQJIcAF4AAANgAABUcKOTgROIAIYAQJIcAF4AAANgAABUcB9DtnWu54JGUKOTgQAAAIYAQJIcAF4AAANgAABUcKOTgQPoAIYAQJIcAF4AAANgAABUcBxTu2uRu4+zgQC3iveBAfGCAaTwgQM=', 'base64')

async function seance(page: Page) {
  const maintenant = () => new Date().toISOString()
  await page.route(`${ESTOMPE}/**`, (route) => {
    const url = new URL(route.request().url())
    if (route.request().method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors })
    switch (url.pathname) {
      case '/me': return json(route, { activated: true, allowed: true, admin: false, rights: ['access'], id: 'me', pseudo: 'moi', sub: 'kc-moi', cinema: true, blocks: ['vocal', 'regarder-ensemble'] })
      case '/unread': return json(route, { channels: [], dms: [], counts: {} })
      case '/me/notify-settings': return json(route, { levels: {}, types: { dm: true, mention: true, channel: true, voice: true }, preview: true, sound: true })
      case '/avatars': case '/statuses': return json(route, {})
      case '/ws-ticket': return json(route, { ticket: 't' })
      case '/servers': return json(route, [serveur])
      case '/servers/s1': return json(route, { server: serveur, channels: [{ id: 'c1', name: 'Cinéma', kind: 'cinema', position: 0 }], members: personnes.map((p) => ({ id: p.user_id, pseudo: p.pseudo, online: true })), roles: [], permissions: ['view_channel', 'connect', 'speak', 'video', 'share_screen', 'organize_watch'] })
      case '/servers/s1/voice': return json(route, { c1: personnes })
      case '/servers/s1/cinema': return json(route, { c1: { now: maintenant(), state: { rev: 3, provider: 'film', video: 'f1', playing: false, pos: 5, rate: 1, at: maintenant(), host: 'me', host_only: false, by_id: 'me', by: 'moi', act: 'pause', queue: [], ready: { me: true, u2: true }, repeat: '', history: [], meta: { title: 'Le Film du soir' } } } })
      case '/servers/s1/watch': case '/servers/s1/music': return json(route, {})
      default: return json(route, route.request().method() === 'GET' ? [] : {})
    }
  })
  await page.route(`${ENTRACTE}/**`, (route) => {
    const url = new URL(route.request().url())
    if (route.request().method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors })
    switch (url.pathname) {
      case '/me': return json(route, { sub: 'kc-moi', name: 'moi', email: '', permissions: { access: true, request: true, admin: false }, push: { web: false, fcm: false } })
      case '/titles/f1/playback': return json(route, { stream_id: 's1', mode: 'direct', url: '/stream/s1/direct', duration: 20, audio: pistes.audio, subtitles: pistes.subtitles.map((t) => ({ ...t, url: `/stream/s1/sub/${t.n}` })), qualities: ['original', '1080p', '720p'], quality: 'original', intro: null })
      case '/stream/s1/direct': return route.fulfill({ status: 200, contentType: 'video/webm', body: FILM_VP9, headers: cors })
      default:
        if (url.pathname.startsWith('/stream/s1/sub/')) return route.fulfill({ status: 200, contentType: 'text/vtt', body: 'WEBVTT\n\n00:00.000 --> 00:20.000\nUne réplique\n', headers: cors })
        return json(route, { items: [], total: 0 })
    }
  })
  await page.goto('/#/s/s1/c1')
  await expect(page.getByRole('heading', { name: 'Cinéma', level: 1 })).toBeVisible()
  await page.evaluate(async (chemin) => {
    const { voice } = await import(chemin)
    voice.channelId = 'c1'
    voice.state = 'connected'
  }, MODULE_VOCAL)
  await expect(page.getByRole('combobox', { name: 'Sous-titres' })).toBeVisible()
  await expect(page.locator('.watch-time')).toContainText('/ 0:20')
}

const pleinEcran = (page: Page) => page.evaluate(() => !!document.fullscreenElement)

const mesurer = (page: Page) => page.evaluate(() => {
  const boite = (s: string) => document.querySelector(s)!.getBoundingClientRect()
  const lecteur = boite('.watch-player')
  const barre = boite('.watch-bar')
  const visibles = (s: string) => [...document.querySelectorAll(s)].map((e) => ({ e, b: e.getBoundingClientRect() })).filter(({ b }) => b.width > 0)
  return {
    vue: { largeur: innerWidth, hauteur: innerHeight },
    lecteur: { largeur: lecteur.width, hauteur: lecteur.height },
    partBarre: barre.height / lecteur.height,
    horsEcran: visibles('.watch-top > *, .watch-bar > *, .watch-voice .vctl > *').filter(({ b }) => b.left < 0 || b.top < 0 || b.right > innerWidth || b.bottom > innerHeight).map(({ e }) => e.getAttribute('aria-label') ?? e.className),
    horsBarre: visibles('.watch-bar > *').filter(({ b }) => b.left < barre.left || b.right > barre.right).map(({ e }) => e.getAttribute('aria-label') ?? e.className),
    hauteurQuitter: boite('.watch-top button').height,
  }
})

for (const [nom, viewport] of [['portrait', { width: 390, height: 844 }], ['paysage', { width: 844, height: 390 }]] as const) {
  test.describe(`téléphone en ${nom}`, () => {
    test.use({ viewport, deviceScaleFactor: 1, hasTouch: true, isMobile: true })

    test('le mode cinéma remplit l’écran, ses commandes tiennent dans la fenêtre et laissent voir le film', async ({ page }) => {
      await seance(page)
      await page.getByRole('button', { name: 'Mode cinéma' }).click()
      await expect(page.getByRole('button', { name: 'Quitter le mode cinéma' })).toBeVisible()
      await expect.poll(() => pleinEcran(page)).toBe(true)
      await expect.poll(async () => (await mesurer(page)).lecteur).toEqual({ largeur: viewport.width, hauteur: viewport.height })
      const m = await mesurer(page)
      expect(m.vue).toEqual({ largeur: viewport.width, hauteur: viewport.height })
      expect(m.horsEcran).toEqual([])
      expect(m.horsBarre).toEqual([])
      expect(m.partBarre).toBeLessThan(0.4)
      expect(m.hauteurQuitter).toBeLessThanOrEqual(44)
      await expect(page.getByRole('alert')).toHaveCount(0)
      for (const nom of ['Piste audio', 'Sous-titres', 'Qualité', 'Préchargement', 'Vitesse']) {
        const choix = page.getByRole('combobox', { name: nom })
        await choix.evaluate((e) => e.scrollIntoView({ block: 'nearest', inline: 'nearest' }))
        await expect(choix).toBeInViewport({ ratio: 1 })
      }
      await page.getByRole('button', { name: 'Quitter le mode cinéma' }).click()
      await expect(page.getByRole('button', { name: 'Mode cinéma', exact: true })).toBeVisible()
      await expect.poll(() => pleinEcran(page)).toBe(false)
    })
  })
}
