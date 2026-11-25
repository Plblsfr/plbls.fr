import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const ARCHIVE = 'mpv-dev-x86_64-20261108-git-55d3feb45c.7z'
const SHA256 = '7c147cb7ceaba717414e782ef4398de728f679db6e86daaa8aeb175670cb7994'
const URL = `https://downloads.sourceforge.net/project/mpv-player-windows/libmpv/${ARCHIVE}`
const DLL = 'libmpv-2.dll'

export function exportsPe(octets) {
  const u16 = (o) => octets.readUInt16LE(o)
  const u32 = (o) => octets.readUInt32LE(o)
  const pe = u32(0x3c)
  if (octets.toString('latin1', pe, pe + 4) !== 'PE\0\0') throw new Error('en-tête PE absent')
  const coff = pe + 4
  const sections = u16(coff + 2)
  const optionnel = coff + 20
  const tailleOptionnel = u16(coff + 16)
  const pe32plus = u16(optionnel) === 0x20b
  const repertoire = optionnel + (pe32plus ? 112 : 96)
  const rvaExports = u32(repertoire)
  if (!rvaExports) return []
  const table = []
  for (let i = 0; i < sections; i++) {
    const s = optionnel + tailleOptionnel + i * 40
    table.push({ va: u32(s + 12), taille: Math.max(u32(s + 8), u32(s + 16)), brut: u32(s + 20) })
  }
  const position = (rva) => {
    const s = table.find((t) => rva >= t.va && rva < t.va + t.taille)
    if (!s) throw new Error(`RVA ${rva} hors des sections`)
    return rva - s.va + s.brut
  }
  const exportsDir = position(rvaExports)
  const nombre = u32(exportsDir + 24)
  const noms = position(u32(exportsDir + 32))
  const resultat = []
  for (let i = 0; i < nombre; i++) {
    const debut = position(u32(noms + i * 4))
    resultat.push(octets.toString('latin1', debut, octets.indexOf(0, debut)))
  }
  return resultat
}

export function fichierDef(noms) {
  return `LIBRARY ${DLL}\nEXPORTS\n${noms.filter((n) => n.startsWith('mpv_')).sort().map((n) => `    ${n}\n`).join('')}`
}

function premierQuiMarche(candidats, args) {
  for (const outil of candidats) {
    try {
      execFileSync(outil, args, { stdio: ['ignore', 'pipe', 'pipe'] })
      return outil
    } catch (e) {
      if (e.code !== 'ENOENT') throw e
    }
  }
  return null
}

async function principal() {
  const racine = resolve(import.meta.dirname, '..', '..')
  const dest = resolve(process.argv[2] ?? join(racine, 'apps', 'estompe', 'natif', 'target', 'mpv'))
  const empreinte = join(dest, 'empreinte')
  const attendus = [DLL, 'mpv.def', 'mpv.lib'].map((f) => join(dest, f))
  if (existsSync(empreinte) && readFileSync(empreinte, 'utf8').trim() === SHA256 && attendus.every(existsSync)) {
    console.log(`libmpv déjà dans ${dest}`)
    return
  }
  const tmp = mkdtempSync(join(tmpdir(), 'mpv-'))
  try {
    const archive = join(tmp, ARCHIVE)
    const res = await fetch(URL)
    if (!res.ok) throw new Error(`téléchargement de libmpv : HTTP ${res.status}`)
    const data = Buffer.from(await res.arrayBuffer())
    const sha = createHash('sha256').update(data).digest('hex')
    if (sha !== SHA256) throw new Error(`somme SHA-256 de libmpv inattendue : ${sha} (attendue ${SHA256})`)
    writeFileSync(archive, data)
    const extrait = join(tmp, 'extrait')
    if (!premierQuiMarche(['7z', '7zz', '7za'], ['x', '-y', `-o${extrait}`, archive, DLL])) {
      throw new Error('7-Zip introuvable (7z, 7zz ou 7za)')
    }
    mkdirSync(dest, { recursive: true })
    copyFileSync(join(extrait, DLL), join(dest, DLL))
    const def = join(dest, 'mpv.def')
    writeFileSync(def, fichierDef(exportsPe(readFileSync(join(dest, DLL)))))
    const llvm = process.env.LLVM_HOME ? [join(process.env.LLVM_HOME, 'bin', 'llvm-lib')] : []
    const args = [`/def:${def}`, '/machine:x64', `/out:${join(dest, 'mpv.lib')}`]
    if (!premierQuiMarche([...llvm, 'llvm-lib', 'lib'], args)) throw new Error('ni llvm-lib ni lib pour la bibliothèque d’import')
    writeFileSync(empreinte, `${SHA256}\n`)
    console.log(`libmpv (${ARCHIVE}) dans ${dest}`)
  } finally {
    rmSync(tmp, { recursive: true, force: true })
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(import.meta.filename)) await principal()
