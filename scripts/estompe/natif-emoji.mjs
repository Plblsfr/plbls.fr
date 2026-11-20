#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { deflateRawSync } from 'node:zlib'

const racine = join(dirname(fileURLToPath(import.meta.url)), '..', '..')
const SORTIE = 'apps/estompe/natif/emoji/twemoji.bin'
const DONNEES = 'https://raw.githubusercontent.com/iamcal/emoji-data/v16.0.0/emoji.json'
const TWEMOJI = 'https://raw.githubusercontent.com/jdecked/twemoji/v17.0.3/assets/svg'
const PAR_BLOC = 32
const EN_PARALLELE = 16
const MAGIQUE = 'EMJ1'

const arguments_ = process.argv.slice(2)
const option = (nom) => {
  const i = arguments_.indexOf(nom)
  return i >= 0 ? arguments_[i + 1] : undefined
}
const cache = option('--cache')

const texte = (unifie) => String.fromCodePoint(...unifie.split('-').map((h) => Number.parseInt(h, 16)))
const sansZeros = (points) => points.map((p) => Number.parseInt(p, 16).toString(16))
const candidats = (unifie) => {
  const points = sansZeros(unifie.split('-'))
  const sans = points.filter((p) => p !== 'fe0f')
  return [...new Set([(points.includes('200d') ? points : sans).join('-'), sans.join('-'), points.join('-')])]
}

async function telecharger(url) {
  for (let essai = 0; essai < 4; essai++) {
    const reponse = await fetch(url)
    if (reponse.status === 404) return null
    if (reponse.ok) return Buffer.from(await reponse.arrayBuffer())
  }
  throw new Error(`${url} : échec répété`)
}

async function svg(nom) {
  const local = cache && join(cache, `${nom}.svg`)
  if (local && existsSync(local)) return readFileSync(local)
  const octets = await telecharger(`${TWEMOJI}/${nom}.svg`)
  if (octets && local) writeFileSync(local, octets)
  return octets
}

if (cache) mkdirSync(cache, { recursive: true })
const donnees = JSON.parse((await telecharger(DONNEES)).toString('utf8'))
const sequences = new Map()
for (const e of donnees) {
  for (const v of [e, ...Object.values(e.skin_variations ?? {})]) {
    const cles = [texte(v.unified)]
    if (v.non_qualified && v.non_qualified.includes('-')) cles.push(texte(v.non_qualified))
    sequences.set(v.unified.toLowerCase(), { cles, candidats: candidats(v.unified) })
  }
}

const noms = [...sequences.keys()].sort()
const fichiers = new Map()
const absents = []
for (let i = 0; i < noms.length; i += EN_PARALLELE) {
  const lot = noms.slice(i, i + EN_PARALLELE)
  const recus = await Promise.all(lot.map(async (nom) => {
    for (const candidat of sequences.get(nom).candidats) {
      const octets = await svg(candidat)
      if (octets) return [nom, octets]
    }
    return [nom, null]
  }))
  for (const [nom, octets] of recus) {
    if (octets) fichiers.set(nom, octets)
    else absents.push(nom)
  }
}

const presents = noms.filter((n) => fichiers.has(n))
const blocs = []
const entrees = []
for (let i = 0; i < presents.length; i += PAR_BLOC) {
  const groupe = presents.slice(i, i + PAR_BLOC)
  let debut = 0
  const morceaux = []
  for (const nom of groupe) {
    const octets = fichiers.get(nom)
    for (const cle of sequences.get(nom).cles) entrees.push({ cle: Buffer.from(cle, 'utf8'), bloc: blocs.length, debut, longueur: octets.length })
    morceaux.push(octets)
    debut += octets.length
  }
  const brut = Buffer.concat(morceaux)
  blocs.push({ compresse: deflateRawSync(brut, { level: 9 }), longueur: brut.length })
}
entrees.sort((a, b) => Buffer.compare(a.cle, b.cle))

const tete = Buffer.alloc(12)
tete.write(MAGIQUE, 0, 'ascii')
tete.writeUInt32LE(entrees.length, 4)
tete.writeUInt32LE(blocs.length, 8)
const index = Buffer.concat(entrees.map((e) => {
  const b = Buffer.alloc(1 + e.cle.length + 10)
  b.writeUInt8(e.cle.length, 0)
  e.cle.copy(b, 1)
  b.writeUInt16LE(e.bloc, 1 + e.cle.length)
  b.writeUInt32LE(e.debut, 3 + e.cle.length)
  b.writeUInt32LE(e.longueur, 7 + e.cle.length)
  return b
}))
let position = 0
const table = Buffer.concat(blocs.map((b) => {
  const t = Buffer.alloc(12)
  t.writeUInt32LE(position, 0)
  t.writeUInt32LE(b.compresse.length, 4)
  t.writeUInt32LE(b.longueur, 8)
  position += b.compresse.length
  return t
}))
const paquet = Buffer.concat([tete, index, table, ...blocs.map((b) => b.compresse)])
mkdirSync(dirname(join(racine, SORTIE)), { recursive: true })
writeFileSync(join(racine, SORTIE), paquet)
const brut = presents.reduce((s, n) => s + fichiers.get(n).length, 0)
console.log(`${SORTIE} : ${presents.length} images, ${entrees.length} séquences, ${blocs.length} blocs, ${(brut / 1e6).toFixed(2)} Mo de SVG → ${(paquet.length / 1e6).toFixed(2)} Mo`)
if (absents.length) console.log(`sans image Twemoji (restent du texte) : ${absents.join(' ')}`)
