// Versions de chaque élément, stockées dans .version à la racine et mises à jour par la CI à chaque fusion dans main :
//  - éléments d'Estompe (api, web, windows, android) et services (security, aegis, entracte, sillon, sablier, regie, tirelire, ui) ;
//  - les éléments concernés sont ceux dont un dossier a changé dans la fusion (ELEMENTS) ; pour un service, un fichier .md ne compte pas ;
//  - fusion d'une branche feat/… = minor +1 (patch remis à 0), tout autre = patch +1, sauf release/… (notes de version) qui ne change rien.
// Le nom de la branche est lu dans le sujet du commit de fusion de GitHub (« Merge pull request #1 from <compte>/feat/x »).
// Les fusions en attente (depuis le dernier commit de main qui a modifié .version) sont recalculées ensemble à chaque fusion.
// Pour une version majeure (ou pour repartir d'un autre numéro), il suffit de modifier la valeur dans .version à la main.
// Un élément absent de .version part de 0.0.0 : sa première fusion feat/ lui donne 0.1.0.

export const SERVICES_RUST = ['security', 'aegis', 'entracte', 'sillon', 'sablier', 'regie', 'tirelire']
export const SERVICES = [...SERVICES_RUST, 'ui']

/** Éléments embarquant chaque dossier (chemins depuis la racine du dépôt, le premier qui correspond l'emporte) : le front est dans les trois interfaces, crates/core dans les apps Tauri, api/commun dans chaque service Rust ; l'app native (abandonnée) ne monte aucune version. */
const ELEMENTS = [
  ['api/commun/', SERVICES_RUST],
  ...SERVICES_RUST.map((s) => [`api/${s}/`, [s]]),
  ['packages/ui/', ['ui']],
  ['api/estompe/', ['api']],
  ['web/estompe/plateforme/', ['web']],
  ['web/estompe/', ['web', 'windows', 'android']],
  ['apps/estompe/natif/', []],
  ['apps/estompe/crates/client/', []],
  ['apps/estompe/crates/plateforme/', []],
  ['apps/estompe/crates/', ['windows', 'android']],
  ['apps/estompe/windows/', ['windows']],
  ['apps/estompe/android/', ['android']],
]

/** @param {string[]} files chemins modifiés par la fusion @returns {string[]} */
export function concerned(files) {
  const out = new Set()
  for (const f of files) {
    const elements = ELEMENTS.find(([dir]) => f.startsWith(dir))?.[1] ?? []
    elements.filter((e) => !(f.endsWith('.md') && SERVICES.includes(e))).forEach((e) => out.add(e))
  }
  return [...out]
}

/** @param {string} v @param {string} subject sujet du commit de fusion */
export function bump(v, subject) {
  if (/\bfrom (?:[\w.-]+\/)?release\//.test(subject)) return v
  const [major, minor, patch] = v.split('.').map(Number)
  return /\bfrom (?:[\w.-]+\/)?feat\//.test(subject) ? `${major}.${minor + 1}.0` : `${major}.${minor}.${patch + 1}`
}

/** @param {Record<string, string>} versions contenu de .version @param {string} subject @param {string[]} files */
export function nextVersions(versions, subject, files) {
  const next = Object.fromEntries(Object.entries(versions).filter(([e]) => e !== 'natif'))
  for (const e of concerned(files)) next[e] = bump(versions[e] ?? '0.0.0', subject)
  return next
}

/** @param {Record<string, string>} versions contenu de .version sur main @param {{ subject: string, files: string[] }[]} fusions de la plus ancienne à la plus récente */
export function pendingVersions(versions, fusions) {
  return fusions.reduce((v, { subject, files }) => nextVersions(v, subject, files), versions)
}
