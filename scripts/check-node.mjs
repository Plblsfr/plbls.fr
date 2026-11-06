import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const major = readFileSync(join(root, '.nvmrc'), 'utf8').trim().replace(/^v/, '').split('.')[0];
const engines = `^${major}`;
const problems = [];

const rootPkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
for (const dir of ['.', ...rootPkg.workspaces]) {
  const pkg = JSON.parse(readFileSync(join(root, dir, 'package.json'), 'utf8'));
  if (pkg.engines?.node !== engines) problems.push(`${join(dir, 'package.json')} : engines.node vaut ${pkg.engines?.node ?? '(absent)'} au lieu de ${engines}`);
}

const scanned = (file) => /(^|\/)(Dockerfile|Makefile|justfile)$/.test(file) || /\.(ya?ml|sh)$/.test(file);
const files = execFileSync('git', ['ls-files'], { cwd: root, encoding: 'utf8' }).split('\n').filter(scanned);
for (const file of files) {
  readFileSync(join(root, file), 'utf8').split('\n').forEach((line, i) => {
    for (const [, image] of line.matchAll(/\bnode:(\d+)/g)) {
      if (image !== major) problems.push(`${file}:${i + 1} : image node:${image} au lieu de node:${major}`);
    }
    if (file.startsWith('.github/workflows/') && /^\s*node-version:/.test(line)) {
      problems.push(`${file}:${i + 1} : utiliser node-version-file: .nvmrc`);
    }
  });
}

if (problems.length) {
  console.error(`Node ${major} attendu partout (.nvmrc) :\n${problems.map((p) => `  ${p}`).join('\n')}`);
  process.exit(1);
}
console.log(`Node ${major} : .nvmrc, engines, images Docker et CI concordent (${files.length} fichiers lus)`);
