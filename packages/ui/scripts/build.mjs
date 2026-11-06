// Build : SCSS → CSS (lisible + minifié), JS → ESM + IIFE (`window.plbls`), types des modules ESM.
// Usage : node scripts/build.mjs [--watch]
import { build, context } from 'esbuild';
import * as sass from 'sass';
import { mkdir, readFile, watch, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(root, 'dist');
const pkg = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
const banner = `/*! @plbls/ui v${pkg.version} — ${pkg.license} */`;

async function css() {
  await mkdir(dist, { recursive: true });
  const entries = { 'plbls-ui': 'plbls-ui.scss', tokens: 'tokens-only.scss' };
  for (const [name, file] of Object.entries(entries)) {
    const path = join(root, 'src/scss', file);
    const expanded = sass.compile(path, { style: 'expanded', charset: false }).css;
    await writeFile(join(dist, `${name}.css`), `${banner}\n${expanded}`);
    const min = sass.compile(path, { style: 'compressed', charset: false }).css;
    await writeFile(join(dist, `${name}.min.css`), `${banner}\n${min}\n`);
  }
}

async function types() {
  await mkdir(dist, { recursive: true });
  const dts = `${banner}\n${await readFile(join(root, 'src/js/index.d.ts'), 'utf8')}`;
  for (const name of ['plbls-ui.esm.d.ts', 'plbls-ui.esm.min.d.ts']) await writeFile(join(dist, name), dts);
}

const jsCommon = { entryPoints: [join(root, 'src/js/index.js')], bundle: true, banner: { js: banner }, target: 'es2020', logLevel: 'warning' };
const jsTargets = [
  { ...jsCommon, format: 'esm', outfile: join(dist, 'plbls-ui.esm.js') },
  { ...jsCommon, format: 'esm', minify: true, outfile: join(dist, 'plbls-ui.esm.min.js') },
  // balise <script> classique : expose window.plbls = { toast, passwordScore, … }
  { ...jsCommon, format: 'iife', globalName: 'plbls', minify: true, outfile: join(dist, 'plbls-ui.js') },
];

if (process.argv.includes('--watch')) {
  await css();
  await types();
  const ctxs = await Promise.all(jsTargets.map((t) => context(t)));
  await Promise.all(ctxs.map((c) => c.watch()));
  console.log('watching…');
  for await (const _ of watch(join(root, 'src/scss'), { recursive: true })) {
    try {
      await css();
      console.log('css rebuilt');
    } catch (e) {
      console.error(e.message);
    }
  }
} else {
  await css();
  await types();
  await Promise.all(jsTargets.map((t) => build(t)));
  console.log('built → dist/');
}
