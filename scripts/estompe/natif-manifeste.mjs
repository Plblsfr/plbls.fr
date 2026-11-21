import { readFileSync, statSync } from 'node:fs'
import { basename } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import { policy } from './update-policy.mjs'

export const CIBLE = 'windows-x86_64'

export function manifeste({ version, notes, politique, url, signature, taille, date = new Date().toISOString() }) {
  return {
    version,
    notes,
    pub_date: date,
    mandatory: politique.mandatory,
    force: politique.force,
    minVersion: politique.minVersion,
    message: politique.message,
    platforms: { [CIBLE]: { url, signature, taille } },
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { version: { type: 'string' }, notes: { type: 'string', default: '' }, installeur: { type: 'string' }, base: { type: 'string' } } })
  if (!values.version || !values.installeur || !values.base) { console.error('usage : natif-manifeste.mjs --version X.Y.Z --installeur <fichier> --base <adresse du dossier> [--notes <texte>]'); process.exit(1) }
  console.log(JSON.stringify(manifeste({
    version: values.version,
    notes: values.notes,
    politique: policy('natif'),
    url: `${values.base}/${basename(values.installeur)}`,
    signature: readFileSync(`${values.installeur}.minisig`).toString('base64'),
    taille: statSync(values.installeur).size,
  }), null, 2))
}
