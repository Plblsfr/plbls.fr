import { execFileSync } from 'node:child_process';
import { mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const root = join(dirname(fileURLToPath(import.meta.url)), '..');
export const ui = join(root, 'packages/ui');

export const destinations = [
  'apps/estompe/frontend/src/vendor/plbls-ui',
  'apps/permission/frontend/src/vendor/plbls-ui',
  'apps/pli/public/vendor/plbls-ui',
];

export const distFiles = ['plbls-ui.esm.min.js', 'plbls-ui.esm.min.d.ts', 'plbls-ui.min.css', 'tokens.min.css'];

const BOM = '﻿';

export function readme(version) {
  return `# @plbls/ui ${version}

Copie générée de \`packages/ui\` par \`npm run ui:sync\` (script \`scripts/sync-ui.mjs\` à la racine du dépôt) : ne pas modifier ces fichiers.
Ils sont dans le dépôt pour que l'application fonctionne hors ligne sans autoriser le CDN dans sa CSP.
La CI (\`npm run ui:check\`) échoue si cette copie diffère de \`packages/ui\`.

- \`plbls-ui.min.css\` : jetons, socle et composants
- \`tokens.min.css\` : jetons seuls (clair et sombre)
- \`plbls-ui.esm.min.js\` et \`plbls-ui.esm.min.d.ts\` : web components, toasts, thème et leurs types
- \`VERSION\` : version de \`@plbls/ui\`
`;
}

export async function expectedFiles(dist, version) {
  const files = new Map();
  for (const name of distFiles) {
    const text = await readFile(join(dist, name), 'utf8');
    if (text.includes(BOM)) throw new Error(`${name} contient un BOM (U+FEFF) : corriger le build de packages/ui`);
    files.set(name, text);
  }
  files.set('VERSION', `${version}\n`);
  files.set('README.md', readme(version));
  return files;
}

export async function compare(dir, files, label = dir) {
  const problems = [];
  let present = [];
  try {
    present = await readdir(dir);
  } catch {
    return [`${label} : dossier absent`];
  }
  for (const [name, content] of files) {
    if (!present.includes(name)) problems.push(`${label}/${name} : absent`);
    else if ((await readFile(join(dir, name), 'utf8')) !== content) problems.push(`${label}/${name} : différent de packages/ui`);
  }
  for (const name of present) if (!files.has(name)) problems.push(`${label}/${name} : fichier en trop`);
  return problems;
}

export async function write(dir, files) {
  await rm(dir, { recursive: true, force: true });
  await mkdir(dir, { recursive: true });
  for (const [name, content] of files) await writeFile(join(dir, name), content);
}

async function main(check) {
  execFileSync(process.execPath, [join(ui, 'scripts/build.mjs')], { stdio: ['ignore', 'ignore', 'inherit'] });
  const { version } = JSON.parse(await readFile(join(ui, 'package.json'), 'utf8'));
  const files = await expectedFiles(join(ui, 'dist'), version);
  if (!check) {
    for (const dest of destinations) await write(join(root, dest), files);
    console.log(`@plbls/ui ${version} copié dans :\n${destinations.map((d) => `  ${d}`).join('\n')}`);
    return;
  }
  const problems = [];
  for (const dest of destinations) problems.push(...(await compare(join(root, dest), files, dest)));
  if (problems.length) {
    console.error(`Copies de @plbls/ui ${version} non à jour (lancer npm run ui:sync) :\n${problems.map((p) => `  ${p}`).join('\n')}`);
    process.exit(1);
  }
  console.log(`@plbls/ui ${version} : ${destinations.length} copies à jour`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) await main(process.argv.includes('--check'));
