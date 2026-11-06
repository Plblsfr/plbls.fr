import { test } from 'node:test';
import assert from 'node:assert/strict';
import { erreursBranche, erreursDescription } from './ci-conventions-pr.mjs';

test('les branches au format <type>/<slug-court> sont acceptées', () => {
  for (const b of ['feat/sondages-multiples', 'fix/films', 'chore/maj-deps', 'docs/ci', 'ci/protection-main', 'ci/version-8f2e438', 'feat/vue-2-colonnes']) {
    assert.deepEqual(erreursBranche(b), [], b);
  }
});

test('les branches de release et de Dependabot sont acceptées', () => {
  for (const b of ['release/api-v1.4.0-app-v2.10.3', 'release/ui-v0.3.0', 'dependabot/npm_and_yarn/npm-1f2e3d']) {
    assert.deepEqual(erreursBranche(b), [], b);
  }
});

test('un préfixe inconnu, des majuscules, des espaces ou un slug vide sont refusés', () => {
  for (const b of ['feature/sondages', 'Feat/sondages', 'feat/Sondages', 'feat/sondages_multiples', 'feat/', 'feat/-x', 'feat/x-', 'feat/a/b', 'main', 'sondages', 'feat/admiring-edison-4dp47F', 'release/1.2.0']) {
    assert.equal(erreursBranche(b).length, 1, b);
  }
});

test('un numéro de ticket dans le nom est refusé', () => {
  for (const b of ['feat/est-189-sondages', 'fix/est189', 'fix/films-est-12']) {
    assert.match(erreursBranche(b)[0], /numéro de ticket/, b);
  }
  assert.deepEqual(erreursBranche('feat/estompe-sondages'), []);
});

test('une description vide, absente ou faite seulement de commentaires est refusée', () => {
  for (const d of [undefined, null, '', '   \n ', '<!-- à remplir -->\n', '<!-<!-- imbriqué -->- à remplir -->']) {
    assert.equal(erreursDescription(d).length, 1, String(d));
  }
  assert.deepEqual(erreursDescription('## Contenu\nAjoute le contrôle.'), []);
});
