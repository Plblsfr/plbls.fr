// @ts-expect-error pas de @types/node dans le projet : le test tourne sous Node via vitest
import { readFileSync } from 'node:fs'
import { expect, it } from 'vitest'

// jsdom ne calcule pas la mise en page : on garde la règle qui évitait la colonne implicite.
it('barre d’actions tactile : placée dans la dernière colonne explicite (bulle « mine » à 1 colonne)', () => {
  const css = readFileSync('src/app.css', 'utf8')
  const rule = css.split('\n').find((l: string) => l.includes('@media (hover: none)') && l.includes('focus-within .msg-tools'))
  expect(rule).toContain('grid-column: -2 / -1')
  expect(rule).not.toMatch(/grid-column: 2\b/)
})

// Sans overflow-x, `overflow-y: auto` rend la liste défilable à l'horizontale dès qu'un élément déborde (EST-16).
it('liste des messages : pas de défilement horizontal', () => {
  const css = readFileSync('src/app.css', 'utf8')
  const rule = css.split('\n').find((l: string) => l.startsWith('.messages {'))
  expect(rule).toContain('overflow-x: hidden')
})

// Mobile : le champ passe sur sa propre ligne, les boutons dessous (EST-20).
it('zone de saisie : champ pleine largeur et options dessous sous 900 px', () => {
  const css = readFileSync('src/app.css', 'utf8')
  const mobile = css.slice(css.indexOf('@media (max-width: 900px)'))
  expect(mobile).toContain('.composer-box { flex-wrap: wrap; }')
  expect(mobile).toMatch(/\.composer textarea \{ order: -1; flex: 1 0 100%;/)
})

// Sans colonne explicite, une citation non coupée élargit la colonne et pousse la réponse hors écran (EST-19).
it('colonne du message : bornée à la largeur, citation limitée à deux lignes', () => {
  const css = readFileSync('src/app.css', 'utf8')
  const col = css.split('\n').find((l: string) => l.startsWith('.msg-col {'))
  expect(col).toContain('grid-template-columns: minmax(0, 1fr)')
  const ref = css.split('\n').find((l: string) => l.startsWith('.reply-ref .reply-text'))
  expect(ref).toContain('-webkit-line-clamp: 2')
})

// Mobile : le burger est la seule porte vers les serveurs et les messages privés, il doit avoir une vraie zone tactile (EST-22).
it('burger : visible sous 900 px, zone tactile de 44 px', () => {
  const css = readFileSync('src/app.css', 'utf8')
  const mobile = css.slice(css.indexOf('@media (max-width: 900px)'))
  expect(mobile).toContain('.burger { display: inline-grid; }')
  expect(css.slice(css.indexOf("@media (pointer: coarse)"))).toContain(".ib { width: 44px; height: 44px; }")
})

// Paramètres : page de taille fixe, le contenu défile à l'intérieur, jamais en largeur (EST-35).
it('paramètres : taille fixe, défilement vertical seulement', () => {
  const css = readFileSync('src/styles/parametres.css', 'utf8')
  expect(css).toMatch(/\.p-corps \{[^}]*min-height: 0; overflow-x: hidden; overflow-y: auto;/)
  expect(css).toMatch(/\.p-corps \.row \{ flex-wrap: wrap;/)
})

// Badge BOT : pilule à hauteur fixe à droite du pseudo, qui se tronque sans masquer le badge (EST-74).
it('badge BOT : sur la ligne du pseudo, hauteur fixe, contraste AA via les jetons du thème', () => {
  const css = readFileSync('src/app.css', 'utf8')
  const line = (start: string) => css.split('\n').find((l: string) => l.startsWith(start))
  expect(line('.msg-head {')).toContain('display: flex')
  const badge = line('.role-badge {')
  expect(badge).toContain('flex: none')
  expect(badge).toContain('height: 16px')
  expect(badge).toContain('background: var(--rose-texte); color: var(--creme)')
})

// Une activité ne doit ni écraser le pseudo, ni faire passer la durée à la ligne, ni changer la hauteur de la ligne (EST-87).
it('liste des membres : hauteur fixe, pseudo borné, durée sur une ligne', () => {
  const css = readFileSync('src/app.css', 'utf8')
  const rule = (sel: string) => css.split('\n').find((l: string) => l.startsWith(sel + ' {')) ?? ''
  expect(rule('.member-row')).toContain('height: 44px')
  expect(rule('.member-pseudo')).toMatch(/min-width: \d+ch/)
  expect(rule('.member-time')).toContain('white-space: nowrap')
  expect(rule('.member-body')).toContain('min-width: 0')
})

// EST-93 : la barre d'actions déborde au-dessus du message au lieu d'en cacher le texte.
it('barre d’actions : ancrée au-dessus du message, dessous si pas de place', () => {
  const css = readFileSync('src/app.css', 'utf8')
  expect(css.split('\n').find((l: string) => l.startsWith('.msg-tools {'))).toContain('bottom: 100%')
  expect(css).toContain('.msg-tools.below { bottom: auto; top: 100%; }')
})

// Une vidéo jointe se charge depuis l'API : sans elle dans media-src, le lecteur reste noir à 0:00 (miniature <img> visible).
it('CSP : media-src autorise l’API (web et app Windows)', () => {
  const media = (csp: string) => csp.split(';').map((d: string) => d.trim()).find((d: string) => d.startsWith('media-src'))
  const tauri = JSON.parse(readFileSync('../../apps/estompe/windows/src-tauri/tauri.conf.json', 'utf8')).app.security.csp
  expect(media(tauri)).toContain('https://chat.plbls.fr')
  expect(media(readFileSync('plateforme/docker/40-estompe-config.sh', 'utf8').match(/CSP="([^"]*)"/)![1])).toContain('$API_URL')
})

// « Regarder ensemble » (EST-149) : les lecteurs de Twitch, Vimeo et Dailymotion et les fichiers directs doivent passer la CSP.
it('CSP : lecteurs Twitch, Vimeo, Dailymotion et fichiers directs autorisés (web et app Windows)', () => {
  const dir = (csp: string, name: string) => csp.split(';').map((d: string) => d.trim()).find((d: string) => d.startsWith(name)) ?? ''
  const tauri = JSON.parse(readFileSync('../../apps/estompe/windows/src-tauri/tauri.conf.json', 'utf8')).app.security.csp
  const web = readFileSync('plateforme/docker/40-estompe-config.sh', 'utf8').match(/CSP="([^"]*)"/)![1]
  for (const csp of [tauri, web]) {
    expect(dir(csp, 'script-src')).toContain('https://player.twitch.tv')
    expect(dir(csp, 'script-src')).toContain('https://player.vimeo.com')
    for (const h of ['https://player.twitch.tv', 'https://clips.twitch.tv', 'https://player.vimeo.com', 'https://www.dailymotion.com', 'https://www.youtube-nocookie.com']) expect(dir(csp, 'frame-src')).toContain(h)
    expect(dir(csp, 'media-src').split(' ')).toContain('https:')
    expect(dir(csp, 'connect-src').split(' ')).toContain('https:')
  }
})

// Le voile du tiroir mobile doit rester sous le tiroir, sinon il capte les clics (salons et serveurs inaccessibles).
it('tiroir mobile : le voile est sous le tiroir', () => {
  const css = readFileSync('src/app.css', 'utf8')
  const mobile = css.slice(css.indexOf('@media (max-width: 900px)'))
  expect(mobile).toMatch(/\.nav-drawer \{[^}]*z-index: var\(--z-barre\);/)
  expect(mobile).toMatch(/\.scrim \{[^}]*z-index: calc\(var\(--z-barre\) - 1\);/)
})

// Colonnes explicites : sans minmax(0, 1fr), un titre ou une miniature large élargit la carte et la fait déborder (EST-166).
it('carte de vidéo : colonnes bornées, miniature 16:9 recadrée sans bandes', () => {
  const css = readFileSync('src/styles/vocal.css', 'utf8')
  const rule = (sel: string) => css.split('\n').find((l: string) => l.startsWith(sel + ' {')) ?? ''
  expect(rule('.vbody-wrap')).toContain('grid-template-columns: minmax(0, 1fr)')
  expect(rule('.vcard')).toContain('grid-template-columns: minmax(0, 1fr)')
  expect(rule('.vthumb')).toContain('aspect-ratio: 16 / 9')
  expect(rule('.vthumb img')).toContain('object-fit: cover')
})
