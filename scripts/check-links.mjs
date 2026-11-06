import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

export const ignored = (file) => /(^|\/)CHANGELOG\.md$/.test(file) || /(^|\/)changelog\.d\//.test(file) || /(^|\/)vendor\//.test(file);

export function targets(markdown) {
  const found = [];
  let fence = null;
  markdown.split('\n').forEach((raw, i) => {
    const marker = raw.match(/^\s*(`{3,}|~{3,})/);
    if (marker) {
      if (!fence) fence = marker[1][0];
      else if (marker[1][0] === fence) fence = null;
      return;
    }
    if (fence) return;
    const line = raw.replace(/(`+)[^`]*?\1/g, '');
    for (const [, target] of line.matchAll(/\]\(\s*<?([^)\s>]+)>?(?:\s+["'(][^)]*)?\)/g)) found.push({ line: i + 1, target });
    const def = line.match(/^\s{0,3}\[[^\]]+\]:\s*<?(\S+?)>?(?:\s|$)/);
    if (def) found.push({ line: i + 1, target: def[1] });
  });
  return found;
}

export const external = (target) => /^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith('//') || target.startsWith('#');

export function resolve(file, target) {
  const path = decodeURIComponent(target.split('#')[0].split('?')[0]);
  return normalize(path.startsWith('/') ? path.slice(1) : join(dirname(file), path));
}

export function broken(base, file, markdown) {
  return targets(markdown)
    .filter(({ target }) => !external(target))
    .map(({ line, target }) => ({ line, target, path: resolve(file, target) }))
    .filter(({ path }) => path.startsWith('..') || !existsSync(join(base, path)))
    .map(({ line, target }) => `${file}:${line} : ${target}`);
}

function main() {
  const files = execFileSync('git', ['ls-files', '-z', '--', '*.md'], { cwd: root, encoding: 'utf8' })
    .split('\0')
    .filter((file) => file && !ignored(file) && existsSync(join(root, file)));
  const problems = files.flatMap((file) => broken(root, file, readFileSync(join(root, file), 'utf8')));
  if (problems.length) {
    console.error(`Liens Markdown internes cassés :\n${problems.map((p) => `  ${p}`).join('\n')}`);
    process.exit(1);
  }
  console.log(`Liens Markdown internes : ${files.length} fichiers vérifiés`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main();
