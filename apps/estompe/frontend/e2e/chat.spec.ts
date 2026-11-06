import { execFileSync } from 'node:child_process'
import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test'
import { E2E } from './global-setup'

// Un petit PNG valide (1×1 pixel) pour tester l'envoi d'images.
const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64')

/** Micro et caméra factices injectés dans la page (indépendant des options de lancement du navigateur). */
function fakeMedia() {
  const md = navigator.mediaDevices
  const devices = [
    { deviceId: 'fake-mic', kind: 'audioinput', label: 'Micro factice', groupId: 'g' },
    { deviceId: 'fake-spk', kind: 'audiooutput', label: 'Sortie factice', groupId: 'g' },
    { deviceId: 'fake-cam', kind: 'videoinput', label: 'Caméra factice', groupId: 'g' },
  ]
  md.enumerateDevices = async () => devices as unknown as MediaDeviceInfo[]
  const real = md.getUserMedia.bind(md)
  md.getUserMedia = async (c?: MediaStreamConstraints) => {
    const out = new MediaStream()
    if (c?.audio) {
      const ctx = new AudioContext()
      const osc = ctx.createOscillator()
      osc.type = 'sawtooth'
      osc.frequency.value = 180
      const g = ctx.createGain()
      g.gain.value = 0.2
      const dest = ctx.createMediaStreamDestination()
      osc.connect(g).connect(dest)
      osc.start()
      out.addTrack(dest.stream.getAudioTracks()[0])
    }
    if (c?.video) {
      const cv = document.createElement('canvas')
      cv.width = 1280
      cv.height = 720
      const x = cv.getContext('2d')!
      let t = 0
      setInterval(() => {
        x.fillStyle = '#246'
        x.fillRect(0, 0, cv.width, cv.height)
        x.fillStyle = '#fc0'
        x.fillRect((t += 12) % 1100, 200, 160, 160)
      }, 66)
      out.addTrack(cv.captureStream(15).getVideoTracks()[0])
    }
    return out.getTracks().length ? out : real(c)
  }
  // Le sélecteur d'écran du système n'existe pas en test : getDisplayMedia renvoie une image animée.
  md.getDisplayMedia = () => md.getUserMedia({ video: true })
}

async function userContext(browser: Browser, name: string): Promise<BrowserContext> {
  const ctx = await browser.newContext({ locale: 'fr-FR', permissions: ['microphone', 'camera'] })
  await ctx.addInitScript(fakeMedia)
  // Le faux gateway lit l'identité dans le cookie de session (la valeur sert d'id et de pseudo).
  await ctx.addCookies([{ name: '__Secure-plbls_sid', value: name, domain: 'localhost', path: '/', secure: true, httpOnly: true, sameSite: 'Lax' }])
  return ctx
}

async function activate(page: Page, pseudo: string) {
  await page.goto(E2E.app)
  await page.getByLabel('Pseudo').fill(pseudo)
  await page.getByRole('button', { name: 'Activer mon compte' }).click()
  await expect(page.getByRole('navigation', { name: 'Serveurs' })).toBeVisible()
}

const composer = (p: Page) => p.getByRole('textbox', { name: /Écrire/ })
async function say(p: Page, text: string) {
  await composer(p).fill(text)
  await composer(p).press('Enter')
}

test.describe.configure({ mode: 'serial' })

let alice: Page
let bob: Page
let serverLink = ''

test.beforeAll(async ({ browser }) => {
  alice = await (await userContext(browser, 'alice')).newPage()
  bob = await (await userContext(browser, 'bob')).newPage()
})

test('sans cookie de session, la page de connexion propose Keycloak', async ({ browser }) => {
  const page = await (await browser.newContext()).newPage()
  await page.goto(E2E.app)
  await expect(page.getByRole('link', { name: 'Se connecter' })).toHaveAttribute('href', /localhost:19000\/auth\/login\?redirect=/)
  await expect(page.getByRole('link', { name: 'Créer un compte' })).toHaveAttribute('href', /\/auth\/register/)
})

test("sans permission, l'accès est refusé", async ({ browser }) => {
  const page = await (await userContext(browser, 'mallory')).newPage()
  await page.goto(E2E.app)
  await expect(page.getByRole('heading', { name: 'Accès non accordé' })).toBeVisible()
})

test("l'activation demande un pseudo valide", async () => {
  await alice.goto(E2E.app)
  await alice.getByLabel('Pseudo').fill('a b')
  await alice.getByLabel('Pseudo').evaluate((el: HTMLInputElement) => el.form!.noValidate = true)
  await alice.getByRole('button', { name: 'Activer mon compte' }).click()
  await expect(alice.getByRole('alert')).toContainText('pseudo')
  await activate(alice, 'alice')
  await expect(alice.getByText('Tu n\'es dans aucun serveur')).toBeVisible()
})

test('créer un serveur, écrire en markdown, modifier et supprimer', async () => {
  await alice.getByRole('navigation', { name: 'Serveurs' }).getByRole('button', { name: 'Créer ou rejoindre un serveur' }).click()
  await alice.getByLabel('Créer un serveur').fill('Amis')
  await alice.getByRole('button', { name: 'Créer', exact: true }).click()
  await expect(alice.getByRole('heading', { name: '# général' })).toBeVisible()

  await say(alice, 'Bonjour **monde** ||secret|| <b>pas du html</b>')
  const msg = alice.locator('.msg').filter({ hasText: 'Bonjour' })
  await expect(msg.locator('strong')).toHaveText('monde')
  await expect(msg.locator('b')).toHaveCount(0) // le HTML brut reste du texte
  await expect(msg).toContainText('<b>pas du html</b>')
  const spoiler = msg.locator('.spoiler')
  await expect(spoiler).not.toHaveClass(/revealed/)
  await spoiler.click()
  await expect(spoiler).toHaveClass(/revealed/)

  await msg.hover()
  await msg.getByRole('button', { name: 'Modifier' }).click()
  await alice.getByLabel('Modifier le message').fill('Bonjour tout le monde')
  await alice.getByLabel('Modifier le message').press('Enter')
  await expect(alice.locator('.msg').filter({ hasText: 'Bonjour tout le monde' })).toContainText('(modifié)')

  await say(alice, 'à supprimer')
  const doomed = alice.locator('.msg').filter({ hasText: 'à supprimer' })
  await doomed.hover()
  await doomed.getByRole('button', { name: 'Supprimer' }).click()
  await alice.getByRole('dialog').getByRole('button', { name: 'Supprimer', exact: true }).click()
  await expect(alice.locator('.msg').filter({ hasText: 'à supprimer' })).toHaveCount(0)
})

test('le glisser à la souris sélectionne le texte des messages, sur plusieurs messages', async () => {
  await say(alice, 'première ligne sélectionnable')
  await expect(alice.locator('.msg-body').filter({ hasText: 'première ligne' })).toBeVisible()
  await say(alice, 'seconde ligne sélectionnable')
  await expect(alice.locator('.msg-body').filter({ hasText: 'seconde ligne' })).toBeVisible()
  const first = alice.locator('.msg-body').filter({ hasText: 'première ligne' })
  const second = alice.locator('.msg-body').filter({ hasText: 'seconde ligne' })
  const a = (await first.boundingBox())!
  const b = (await second.boundingBox())!
  await alice.mouse.move(a.x + 1, a.y + a.height / 2)
  await alice.mouse.down()
  await alice.mouse.move(b.x + b.width - 1, b.y + b.height / 2, { steps: 10 })
  await alice.mouse.up()
  const sel = await alice.evaluate(() => getSelection()!.toString())
  expect(sel).toContain('première ligne sélectionnable')
  expect(sel).toContain('seconde ligne sélectionnable')
  // appuyer sur du texte déjà sélectionné efface l'ancienne sélection (sinon : glisser-déposer natif)
  await alice.mouse.move(a.x + 1, a.y + a.height / 2)
  await alice.mouse.down()
  await alice.mouse.move(a.x + 80, a.y + a.height / 2, { steps: 5 })
  await alice.mouse.up()
  const again = await alice.evaluate(() => getSelection()!.toString())
  expect(again).not.toContain('seconde') // l'ancienne sélection ne subsiste pas
})

test('un lien d\'invitation fait rejoindre le serveur, les membres se voient', async () => {
  await alice.getByRole('button', { name: 'Paramètres du serveur' }).click()
  await alice.getByRole('button', { name: 'Créer un lien' }).click()
  serverLink = await alice.locator('plbls-copy-field input').first().inputValue()
  expect(serverLink).toMatch(/#\/join\/[A-Z0-9]{5}-[A-Z0-9]{5}$/)
  await alice.keyboard.press('Escape')

  await activate(bob, 'bob')
  await bob.goto(serverLink)
  await expect(bob.getByRole('heading', { name: '# général' })).toBeVisible()
  await expect(bob.locator('.msg').filter({ hasText: 'Bonjour tout le monde' })).toBeVisible() // historique

  // Alice voit Bob arriver et passer en ligne sans recharger
  await expect(alice.locator('.members')).toContainText('bob')
  await expect(alice.locator('.members .nav-item').filter({ hasText: 'bob' }).locator('.dot')).toHaveClass(/online/)
})

test('les messages, mentions et réactions arrivent en temps réel', async () => {
  // Alice regarde l'accueil : un message de Bob qui la mentionne fait apparaître une pastille sur le serveur
  await alice.getByRole('link', { name: 'Messages privés' }).click()
  await say(bob, 'Salut @alice, ça va ?')
  await expect(alice.getByRole('link', { name: 'Amis' }).locator('.pip')).toHaveText('1')

  await alice.getByRole('link', { name: 'Amis' }).click()
  const hello = alice.locator('.msg').filter({ hasText: 'Salut' })
  await expect(hello).toBeVisible()
  await expect(hello.locator('.mention')).toHaveText('@alice')
  await expect(alice.getByRole('link', { name: 'Amis' }).locator('.pip')).toHaveCount(0) // mention lue en ouvrant le salon

  // Réaction d'Alice, vue par Bob
  await hello.hover()
  await hello.getByRole('button', { name: 'Réagir avec 👍' }).click()
  await expect(hello.locator('.react.me')).toContainText('1')
  await expect(bob.locator('.msg').filter({ hasText: 'Salut' }).locator('.react')).toContainText('👍 1')
  await expect(bob.locator('.msg').filter({ hasText: 'Salut' }).locator('.react.me')).toHaveCount(0)
})

test('les messages privés et les fichiers', async () => {
  // Alice écrit à Bob depuis la liste des membres
  await alice.locator('.members .nav-item').filter({ hasText: 'bob' }).click()
  await expect(alice.getByRole('heading', { name: '@ bob' })).toBeVisible()
  await say(alice, 'Un message privé')

  await expect(bob.getByRole('link', { name: 'Messages privés' }).locator('.pip')).toBeVisible()
  await bob.getByRole('link', { name: 'Messages privés' }).click()
  await bob.getByRole('link', { name: /alice/ }).click()
  await expect(bob.locator('.msg').filter({ hasText: 'Un message privé' })).toBeVisible()

  // Image + document
  await alice.locator('input[type=file]').setInputFiles([
    { name: 'pixel.png', mimeType: 'image/png', buffer: PNG },
    { name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('bonjour') },
  ])
  await expect(alice.locator('.pending .file-chip')).toHaveCount(2)
  await expect(alice.locator('.pending')).not.toContainText('envoi…')
  await expect(alice.getByRole('button', { name: 'Envoyer' })).toBeEnabled()
  await alice.getByRole('button', { name: 'Envoyer' }).click()

  const withFiles = bob.locator('.msg').filter({ has: bob.locator('img') })
  const img = withFiles.locator('img')
  await expect(img).toBeVisible()
  await expect.poll(() => img.evaluate((i: HTMLImageElement) => i.naturalWidth)).toBeGreaterThan(0)
  await expect(withFiles.getByRole('link', { name: /notes\.txt/ })).toBeVisible()
  await alice.getByRole('link', { name: 'Amis' }).click()
  await expect(alice.getByRole('heading', { name: '# général' })).toBeVisible()
  await alice.screenshot({ path: 'test-results/desktop.png' })
  await bob.screenshot({ path: 'test-results/desktop-dm.png' })
})

test('un utilisateur extérieur ne voit ni le serveur ni les messages', async ({ browser }) => {
  const carol = await (await userContext(browser, 'carol')).newPage()
  await activate(carol, 'carol')
  await expect(carol.getByRole('link', { name: 'Amis' })).toHaveCount(0)
  await carol.goto(E2E.app + '/#/s/00000000-0000-0000-0000-000000000000')
  await expect(carol.getByText("Tu n'es dans aucun serveur")).toBeVisible()
})

test('le serveur est utilisable au téléphone', async ({ browser }) => {
  const ctx = await userContext(browser, 'alice')
  const phone = await ctx.newPage()
  await phone.setViewportSize({ width: 390, height: 800 })
  await phone.goto(E2E.app)
  await expect(phone.getByRole('button', { name: 'Menu' })).toBeVisible()
  await phone.getByRole('button', { name: 'Menu' }).click()
  await phone.waitForTimeout(400)
  await phone.screenshot({ path: 'test-results/mobile-menu.png' })
  await phone.getByRole('link', { name: 'Amis' }).click()
  await expect(phone.locator('.app')).toHaveAttribute('data-nav', 'closed') // le menu se referme après le choix
  await phone.waitForTimeout(400) // fin de la transition
  await expect(phone.getByRole('heading', { name: '@ bob' }).or(phone.getByRole('heading', { name: '# général' }))).toBeVisible()
  expect(await phone.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true)
  await phone.screenshot({ path: 'test-results/mobile.png' })
})

test('répondre, épingler et rechercher un message', async () => {
  await say(alice, 'Rendez-vous à 20h devant le cinéma')
  const target = alice.locator('.msg').filter({ has: alice.locator('.msg-body', { hasText: 'Rendez-vous à 20h' }) }) // le texte cité n'est pas dans .msg-body
  await target.hover()
  await target.getByRole('button', { name: 'Répondre' }).click()
  await expect(alice.locator('.reply-bar')).toContainText('Rendez-vous')
  await say(alice, 'ok pour moi')
  await expect(alice.locator('.reply-ref').filter({ hasText: 'Rendez-vous' })).toBeVisible()
  await expect(alice.locator('.reply-bar')).toHaveCount(0)

  await target.hover()
  await target.getByRole('button', { name: 'Épingler' }).click()
  await expect(target.locator('.pinned')).toBeVisible()
  await alice.getByRole('button', { name: 'Messages épinglés' }).click()
  await expect(alice.getByRole('complementary', { name: 'Messages épinglés' })).toContainText('Rendez-vous à 20h')
  await alice.getByRole('button', { name: 'Messages épinglés' }).click()

  await alice.getByRole('button', { name: 'Rechercher' }).click()
  await alice.getByRole('searchbox', { name: 'Rechercher' }).fill('cinéma')
  await alice.getByRole('searchbox', { name: 'Rechercher' }).press('Enter')
  await expect(alice.getByRole('region', { name: 'Résultats' })).toContainText('devant le cinéma')
})

test("l'indicateur « écrit… » et le brouillon par conversation", async () => {
  await bob.goto(E2E.app)
  await bob.getByRole('link', { name: 'Amis' }).click()
  await bob.getByRole('link', { name: /général/ }).first().click()
  await expect(composer(bob)).toBeVisible()
  await alice.waitForTimeout(3100) // le signal « écrit… » est limité à un toutes les 3 s
  await composer(alice).pressSequentially('un message à moitié écrit')
  await expect(bob.locator('.typing')).toHaveText("alice est en train d'écrire…")
  await expect(alice.locator('.typing')).toHaveText('') // on ne se voit pas soi-même
  // le brouillon survit à un changement de conversation
  await alice.getByRole('link', { name: /vocal/ }).first().click()
  await alice.getByRole('link', { name: /général/ }).first().click()
  await expect(composer(alice)).toHaveValue('un message à moitié écrit')
  await composer(alice).fill('')
})

test('une longue conversation reste fluide : seules les lignes proches de l\'écran sont dans le DOM', async () => {
  const sql = `INSERT INTO messages (channel_id, author_id, content)
    SELECT (SELECT id FROM channels WHERE kind='text' ORDER BY created_at LIMIT 1), (SELECT id FROM users WHERE pseudo='alice'),
           'message de masse numéro ' || g FROM generate_series(1, 600) g`
  execFileSync('docker', ['exec', 'estompe-e2e-pg', 'psql', '-U', 'postgres', 'estompe', '-c', sql], { stdio: 'ignore' })
  await alice.reload()
  await expect(alice.locator('.msg').filter({ hasText: 'message de masse numéro 600' })).toBeVisible()
  // en remontant, l'historique se charge ; le DOM reste borné et les messages éloignés en sont retirés
  for (let i = 0; i < 7; i++) {
    await alice.locator('.messages').evaluate((el) => (el.scrollTop = 0))
    await alice.waitForTimeout(300)
  }
  expect(await alice.locator('.msg').count()).toBeLessThan(150)
  await expect(alice.locator('.msg').filter({ hasText: 'message de masse numéro 600' })).toHaveCount(0)
  // et en redescendant, le dernier message revient
  await alice.locator('.messages').evaluate((el) => (el.scrollTop = el.scrollHeight))
  await expect(alice.locator('.msg').filter({ hasText: 'message de masse numéro 600' })).toBeVisible()
})

test('mesure : démarrage et mémoire JS avec une longue conversation', async () => {
  const cdp = await alice.context().newCDPSession(alice)
  await cdp.send('Performance.enable')
  const t0 = Date.now()
  await alice.reload()
  await expect(alice.locator('.msg').filter({ hasText: 'message de masse numéro 600' })).toBeVisible()
  const ready = Date.now() - t0
  for (let i = 0; i < 7; i++) {
    await alice.locator('.messages').evaluate((el) => (el.scrollTop = 0))
    await alice.waitForTimeout(250)
  }
  const { metrics } = await cdp.send('Performance.getMetrics')
  const heap = metrics.find((m) => m.name === 'JSHeapUsedSize')!.value / 1048576
  const nodes = await alice.evaluate(() => document.querySelectorAll('*').length) // éléments vivants (la métrique « Nodes » compte aussi ceux qui attendent le ramasse-miettes)
  console.log(`MESURE : application prête en ${ready} ms · tas JS ${heap.toFixed(1)} Mo · ${nodes} nœuds DOM (350 messages chargés)`)
  expect(ready).toBeLessThan(3000)
  expect(heap).toBeLessThan(80)
  expect(nodes).toBeLessThan(3000)
})

test.describe('accessibilité (axe-core)', () => {
  async function audit(page: Page, label: string) {
    const { default: AxeBuilder } = await import('@axe-core/playwright')
    const res = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
    const bad = res.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical')
    const report = bad.map((v) => `${v.id} (${v.impact}) : ${v.help}\n   ${v.nodes.slice(0, 3).map((n) => n.target.join(' ')).join('\n   ')}`).join('\n')
    expect(bad, `${label} :\n${report}`).toEqual([])
  }

  test('la conversation, les paramètres et le salon vocal', async () => {
    await alice.goto(E2E.app)
    await alice.getByRole('link', { name: 'Amis' }).click()
    await alice.getByRole('link', { name: /général/ }).first().click()
    await expect(composer(alice)).toBeVisible()
    await audit(alice, 'conversation')
    await alice.getByRole('button', { name: 'Paramètres', exact: true }).click()
    await expect(alice.getByRole('dialog')).toBeVisible()
    await audit(alice, 'paramètres')
    await alice.keyboard.press('Escape')
    await alice.getByRole('link', { name: /vocal/ }).first().click()
    // le clic connecte automatiquement : selon l'environnement, on voit la scène connectée ou « Rejoindre le salon » (échec de connexion)
    const quit = alice.getByRole('button', { name: 'Quitter le vocal' })
    await expect(alice.getByRole('button', { name: 'Rejoindre le salon' }).or(quit)).toBeVisible({ timeout: 20_000 })
    await audit(alice, 'salon vocal')
    if (await quit.isVisible()) await quit.click()
  })
})

test.describe('vocal (vrai LiveKit, micro et caméra factices)', () => {
  const join = (p: Page) => p.getByRole('button', { name: 'Rejoindre le salon' })

  test('deux personnes, caméra, qualité, sourdine casque, détection de voix', async () => {
    for (const p of [alice, bob]) {
      await p.goto(E2E.app)
      await p.getByRole('link', { name: 'Amis' }).click()
      await p.getByRole('link', { name: /vocal/ }).first().click()
    }
    // un clic sur le salon vocal suffit : la connexion est automatique
    await expect(alice.getByRole('button', { name: /Couper le micro/ })).toBeVisible({ timeout: 20_000 })
    await expect(bob.getByRole('button', { name: /Couper le micro/ })).toBeVisible({ timeout: 20_000 })
    await expect(alice.locator('.stage .ptile')).toHaveCount(2, { timeout: 20_000 })

    // la qualité de connexion de l'autre apparaît
    await expect(alice.locator('.stage .ptile .quality').first()).toBeVisible({ timeout: 15_000 })

    // caméra : bob la publie, alice voit la tuile avec sa résolution
    await bob.getByRole('button', { name: /Activer la caméra/ }).click()
    await expect(bob.locator('.video-tile')).toHaveCount(1, { timeout: 15_000 })
    await expect(alice.locator('.video-tile')).toHaveCount(1, { timeout: 20_000 })
    await expect(alice.locator('.video-tile figcaption')).toContainText(/bob.*caméra/, { timeout: 15_000 })
    await expect(alice.locator('.video-tile figcaption')).toContainText(/\d+p/, { timeout: 15_000 })

    // mise en grand, puis détachement
    await alice.locator('.video-tile').hover()
    await alice.getByRole('button', { name: 'Mettre en grand' }).click()
    await expect(alice.locator('.stage-main .video-tile')).toHaveCount(1)
    await alice.locator('.stage-main .video-tile').hover()
    await alice.getByRole('button', { name: 'Détacher' }).click()
    await expect(alice.locator('.stage-main')).toHaveCount(0)

    // sourdine casque : l'autre voit l'indicateur
    const bobTile = alice.locator('.stage .ptile').filter({ hasText: 'bob' })
    await bob.getByRole('button', { name: 'Sourdine casque' }).click()
    await expect(bobTile.getByRole('img', { name: 'Sourdine casque' })).toBeVisible({ timeout: 10_000 })
    await bob.getByRole('button', { name: 'Réactiver le son' }).click()
    await expect(bobTile.getByRole('img', { name: 'Sourdine casque' })).toHaveCount(0, { timeout: 10_000 })

    // micro coupé : indicateur visible
    await bob.getByRole('button', { name: /Couper le micro/ }).click()
    await expect(bobTile.getByRole('img', { name: 'Micro coupé' })).toBeVisible({ timeout: 10_000 })
    await bob.getByRole('button', { name: /Réactiver le micro/ }).click()

    // le niveau du micro (signal factice) est mesuré : la jauge bouge, le traitement RNNoise tourne
    await expect.poll(async () => Number(await bob.getByRole('meter').getAttribute('aria-valuenow')), { timeout: 15_000 }).toBeGreaterThan(-80)

    // caméra : changement de qualité à chaud sans erreur
    await bob.getByRole('button', { name: 'Paramètres', exact: true }).click()
    await bob.getByLabel('Qualité de la caméra').selectOption('360p')
    await bob.keyboard.press('Escape')
    await expect(bob.locator('.video-tile')).toHaveCount(1, { timeout: 15_000 })
    await expect(bob.locator('.plbls-alert--erreur')).toHaveCount(0)

    await bob.getByRole('button', { name: 'Quitter le vocal' }).click()
    await alice.getByRole('button', { name: 'Quitter le vocal' }).click()
  })

  test('un partage d\'écran est reçu par les autres participants, qui choisissent de le regarder', async () => {
    for (const p of [alice, bob]) {
      await p.goto(E2E.app)
      await p.getByRole('link', { name: 'Amis' }).click()
      await p.getByRole('link', { name: /vocal/ }).first().click()
      await expect(p.getByRole('button', { name: /Couper le micro/ })).toBeVisible({ timeout: 20_000 })
    }
    await bob.getByRole('button', { name: "Partager l'écran" }).click()
    await bob.getByRole('button', { name: 'Partager', exact: true }).click()
    await expect(bob.getByRole('button', { name: 'Arrêter le partage' })).toBeVisible({ timeout: 15_000 })
    await expect(bob.locator('.video-tile')).toHaveCount(1, { timeout: 15_000 })

    // alice voit le stream de bob sans le recevoir, jusqu'à ce qu'elle choisisse de le regarder
    const regarder = alice.getByRole('button', { name: 'Regarder', exact: true })
    await expect(regarder).toBeVisible({ timeout: 20_000 })
    await regarder.click()
    await expect(alice.locator('.video-tile figcaption')).toContainText(/bob.*écran/, { timeout: 20_000 })

    await alice.getByRole('button', { name: 'Arrêter de regarder' }).click()
    await expect(alice.locator('.video-tile')).toHaveCount(0, { timeout: 10_000 })
    await expect(alice.getByRole('button', { name: 'Regarder', exact: true })).toBeVisible()

    await bob.getByRole('button', { name: 'Arrêter le partage' }).click()
    await bob.getByRole('button', { name: 'Quitter le vocal' }).click()
    await alice.getByRole('button', { name: 'Quitter le vocal' }).click()
  })

  test('détection de voix : le micro ne s\'ouvre que sous le seuil, le seuil se règle', async () => {
    await alice.getByRole('button', { name: 'Paramètres', exact: true }).click()
    await alice.getByLabel('Réduction de bruit').selectOption('standard') // RNNoise traiterait le signal de test comme du bruit
    await alice.getByLabel('Activation du micro').selectOption('vad')
    await alice.keyboard.press('Escape')
    await join(alice).click()
    await expect(alice.getByText(/Seuil de voix/)).toBeVisible({ timeout: 20_000 })
    await expect(alice.getByLabel('Seuil de détection de voix')).toBeVisible()
    // seuil très bas : tout dépasse le seuil, le micro s'ouvre ; seuil très haut : il reste fermé
    await alice.getByLabel('Seuil de détection de voix').fill('-75')
    await expect(alice.getByText(/voix détectée/)).toBeVisible({ timeout: 10_000 })
    await alice.getByLabel('Seuil de détection de voix').fill('-15')
    await expect(alice.getByText(/silence/)).toBeVisible({ timeout: 10_000 })
    await alice.getByRole('button', { name: 'Quitter le vocal' }).click()
  })
})
