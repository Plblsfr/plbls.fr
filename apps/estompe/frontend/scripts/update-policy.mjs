// Politique de mise à jour d'une plateforme, lue dans backend/core/updates.json (le même fichier que l'API pour son refus 426).
// Usage : node scripts/update-policy.mjs <web|android|windows> → JSON fusionné dans latest.json, android.json et web.json.
// `force` reprend `mandatory` : les anciens clients ne lisent que ce champ.
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const FILE = new URL('../../backend/core/updates.json', import.meta.url)

export function policy(platform, src = JSON.parse(readFileSync(FILE, 'utf8'))) {
  const { platforms = {}, ...common } = src
  const p = { mandatory: false, minVersion: '', message: '', ...common, ...platforms[platform] }
  return { mandatory: p.mandatory === true, minVersion: String(p.minVersion), message: String(p.message), force: p.mandatory === true }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const platform = process.argv[2]
  if (!['web', 'android', 'windows'].includes(platform)) { console.error('plateforme : web, android ou windows'); process.exit(1) }
  console.log(JSON.stringify(policy(platform)))
}
