// Versions de chaque élément (api, web, windows, android), stockées dans .version à la racine et mises à jour par la CI à chaque fusion dans main :
//  - les éléments concernés sont ceux dont un dossier a changé dans la fusion (ELEMENTS) ;
//  - fusion d'une branche feat/… = minor +1 (patch remis à 0), tout autre = patch +1, sauf release/… (notes de version) qui ne change rien.
// Le nom de la branche est lu dans le sujet du commit de fusion de GitHub (« Merge pull request #1 from <compte>/feat/x »).
// Pour une version majeure (ou pour repartir d'un autre numéro), il suffit de modifier la valeur dans .version à la main.

/** Éléments embarquant chaque dossier (chemins depuis la racine du dépôt, le premier qui correspond l'emporte) : le front est dans les trois interfaces, crates/ dans les apps natives. */
const ELEMENTS = [
  ['api/estompe/', ['api']],
  ['web/estompe/plateforme/', ['web']],
  ['web/estompe/', ['web', 'windows', 'android']],
  ['apps/estompe/crates/', ['windows', 'android']],
  ['apps/estompe/windows/', ['windows']],
  ['apps/estompe/android/', ['android']],
]

/** @param {string[]} files chemins modifiés par la fusion @returns {string[]} */
export function concerned(files) {
  const out = new Set()
  for (const f of files) ELEMENTS.find(([dir]) => f.startsWith(dir))?.[1].forEach((e) => out.add(e))
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
  const next = { ...versions }
  for (const e of concerned(files)) next[e] = bump(versions[e], subject)
  return next
}
