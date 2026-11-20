#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const racine = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SOURCES = 'web/estompe/src';
const MATRICE = 'docs/estompe/natif-parite.md';
const ETATS = ['absent', 'en cours', 'fait', 'écart assumé'];
const ENSEMBLES = ['blocks', 'sections'];

function composants(dossier) {
  return readdirSync(join(racine, SOURCES, dossier), { withFileTypes: true }).flatMap((e) => {
    const chemin = dossier ? `${dossier}/${e.name}` : e.name;
    if (e.isDirectory()) return chemin === 'vendor' ? [] : composants(chemin);
    return e.name.endsWith('.svelte') ? [chemin] : [];
  });
}

function dossiers(parent) {
  return readdirSync(join(racine, SOURCES, parent), { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => `${parent}/${e.name}/`);
}

function lignes(markdown) {
  return markdown.split('\n').flatMap((brut, i) => {
    if (!brut.startsWith('|')) return [];
    const cellules = brut.split('|').slice(1, -1).map((c) => c.trim());
    if (cellules.every((c) => /^:?-+:?$/.test(c)) || cellules[3] === 'État') return [];
    const chemin = cellules[0]?.match(/^`([^`]+)`$/)?.[1];
    const cites = [...brut.matchAll(/`((?:apps|web)\/[^`]+)`/g)].map((m) => m[1]).filter((c) => c !== chemin);
    return [{ numero: i + 1, element: cellules[0] ?? '', chemin, cites, etat: cellules[3], colonnes: cellules.length }];
  });
}

const emplacement = (chemin) => join(racine, /^(apps|web)\//.test(chemin) ? chemin : `${SOURCES}/${chemin}`);

function existe(chemin) {
  const cible = emplacement(chemin);
  return existsSync(cible) && statSync(cible).isDirectory() === chemin.endsWith('/');
}

function controler() {
  const table = lignes(readFileSync(join(racine, MATRICE), 'utf8'));
  const chemins = table.filter((l) => l.chemin).map((l) => l.chemin);
  const connus = new Set(chemins);
  const tous = composants('').sort();
  const ecarts = [
    ['Composants sans ligne dans la matrice', tous.filter((c) => !connus.has(c))],
    ['Blocs et sections sans ligne dans la matrice', ENSEMBLES.flatMap(dossiers).filter((d) => !connus.has(d))],
    ["Lignes dont le fichier ou le dossier n'existe plus", table.flatMap((l) => [l.chemin, ...l.cites].filter((c) => c && !existe(c)).map((c) => `ligne ${l.numero} : ${c}`))],
    ['Lignes en double', [...new Set(chemins.filter((c, i) => chemins.indexOf(c) !== i))]],
    ['Lignes mal formées (cinq colonnes attendues : élément, lot, issue, état, note)', table.filter((l) => l.colonnes !== 5).map((l) => `ligne ${l.numero} : ${l.element}`)],
    [`États hors de la liste autorisée (${ETATS.join(', ')})`, table.filter((l) => l.colonnes === 5 && !ETATS.includes(l.etat)).map((l) => `ligne ${l.numero} : ${l.element} : « ${l.etat} »`)],
  ].filter(([, liste]) => liste.length);
  return { table, tous, ecarts };
}

const { table, tous, ecarts } = controler();
if (ecarts.length) {
  console.error(`Matrice de parité (${MATRICE}) : écart avec ${SOURCES}/`);
  for (const [titre, liste] of ecarts) console.error(`\n${titre} :\n${liste.map((l) => `  ${l}`).join('\n')}`);
  console.error(`\nAjouter ou corriger les lignes dans ${MATRICE} (règle en tête du fichier).`);
  process.exit(1);
}
const compte = (liste) => ETATS.map((e) => `${e} ${liste.filter((l) => l.etat === e).length}`).join(', ');
const estComposant = (l) => l.chemin?.endsWith('.svelte') && !/^(apps|web)\//.test(l.chemin);
console.log(`Matrice de parité : ${tous.length} composants, tous présents (${compte(table.filter(estComposant))})`);
console.log(`Toutes les lignes (${table.length}) : ${compte(table)}`);
