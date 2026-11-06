import { fileURLToPath } from 'node:url';

const types = ['feat', 'fix', 'chore', 'docs', 'ci'];
const slug = '[a-z0-9]+(?:-[a-z0-9]+)*';
const version = 'v\\d+\\.\\d+\\.\\d+';
const formats = [
  new RegExp(`^(?:${types.join('|')})/${slug}$`),
  new RegExp(`^release/api-${version}-app-${version}$`),
  new RegExp(`^release/[a-z0-9-]+-${version}$`),
];
const dispensees = [/^dependabot\//];

export function erreursBranche(branche) {
  if (dispensees.some((re) => re.test(branche))) return [];
  if (!formats.some((re) => re.test(branche))) {
    return [
      `branche « ${branche} » hors convention : <type>/<slug-court>, type parmi ${types.join(', ')} ` +
        '(ex. feat/sondages-multiples), slug en minuscules séparé par des tirets ; ' +
        'release/api-vA.B.C-app-vX.Y.Z ou release/<composant>-vX.Y.Z pour une release',
    ];
  }
  if (/(?:^|[/-])est-?\d+(?:-|$)/.test(branche)) {
    return [`branche « ${branche} » : pas de numéro de ticket dans le nom (la clé EST-<n> va dans le titre et les commits)`];
  }
  return [];
}

export function erreursDescription(description) {
  const texte = (description ?? '').replace(/<!--[\s\S]*?-->/g, '').trim();
  return texte ? [] : ['la PR n’a pas de description : contenu, vérifications faites et ce qui n’a pas été vérifié'];
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const erreurs = [...erreursBranche(process.env.BRANCHE ?? ''), ...erreursDescription(process.env.DESCRIPTION)];
  for (const e of erreurs) console.log(`::error::${e}`);
  if (erreurs.length) process.exit(1);
  console.log(`Branche ${process.env.BRANCHE} et description conformes`);
}
