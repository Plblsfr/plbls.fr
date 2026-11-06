import assert from "node:assert/strict";
import { test } from "node:test";
import deepmerge from "./index.js";

const malveillant = () => JSON.parse('{"__proto__":{"pollue":true},"a":1}');

test("une clé __proto__ ne change jamais le prototype du résultat", () => {
  for (const cible of [{}, Object.create(null), null, undefined, 5, "x", true, { a: 0 }]) {
    const resultat = deepmerge(cible, malveillant());
    assert.equal(Object.getPrototypeOf(resultat), Object.prototype);
    assert.equal(resultat.pollue, undefined);
    assert.equal(resultat.a, 1);
  }
  assert.equal(deepmerge.all([malveillant(), malveillant()]).pollue, undefined);
  assert.equal(deepmerge({ b: {} }, { b: malveillant() }).b.pollue, undefined);
  assert.equal({}.pollue, undefined);
});

test("fusion profonde comme deepmerge 4.3.1", () => {
  const x = { a: { b: 1, l: [1] }, d: new Date(0), s: Symbol.for("s") };
  const y = { a: { c: 2, l: [2] }, e: /r/ };
  const r = deepmerge(x, y);
  assert.deepEqual(r.a, { b: 1, c: 2, l: [1, 2] });
  assert.equal(r.d, x.d);
  assert.equal(r.e, y.e);
  assert.notEqual(r.a, x.a);
  assert.deepEqual(deepmerge([1], [2]), [1, 2]);
  assert.deepEqual(deepmerge({ l: [1] }, { l: [2] }, { arrayMerge: (_, s) => s }), { l: [2] });
  assert.deepEqual(deepmerge({ k: { a: 1 } }, { k: { b: 2 } }, { customMerge: (k) => (k === "k" ? (_, b) => b : undefined) }), { k: { b: 2 } });
  assert.equal(deepmerge({ a: x.a }, {}, { clone: false }).a, x.a);
  const sym = Symbol("t");
  assert.equal(deepmerge({}, { [sym]: 1 })[sym], 1);
  assert.throws(() => deepmerge.all({}), /first argument should be an array/);
});
