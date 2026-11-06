import { test } from 'vitest';
import assert from 'node:assert/strict';
import { passwordScore } from './password-strength.js';

test('score de 0 à 4 selon la longueur, la casse, les chiffres et les caractères spéciaux', () => {
  assert.equal(passwordScore(''), 0);
  assert.equal(passwordScore('abcdefgh'), 1);
  assert.equal(passwordScore('abcdefghijkl'), 2);
  assert.equal(passwordScore('Abcdefghijkl'), 3);
  assert.equal(passwordScore('Abcdefghijk1!'), 4);
});

test('la balise se met à jour avec sa valeur', async () => {
  const el = document.createElement('plbls-password-strength');
  document.body.append(el);
  el.setAttribute('value', 'Abcdefghijk1!');
  assert.match(el.textContent, /Excellent/);
  el.remove();
});
