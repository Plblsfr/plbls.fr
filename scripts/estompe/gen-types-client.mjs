import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { parse } from 'yaml'

const SOURCE = new URL('../../docs/estompe/openapi.yaml', import.meta.url)
const SORTIE = new URL('../../apps/estompe/crates/client/src/genere/types.rs', import.meta.url)
const COMMANDE = 'node scripts/estompe/gen-types-client.mjs'

const METHODES = ['get', 'post', 'put', 'patch', 'delete']
const LARGEUR = 100
const JSON_BRUT = 'serde_json::Value'
const MOTS_RESERVES = new Set([
  'abstract', 'as', 'async', 'await', 'become', 'box', 'break', 'const', 'continue', 'do', 'dyn', 'else', 'enum', 'extern', 'false', 'final', 'fn', 'for', 'gen',
  'if', 'impl', 'in', 'let', 'loop', 'macro', 'match', 'mod', 'move', 'mut', 'override', 'priv', 'pub', 'ref', 'return', 'static', 'struct', 'trait', 'true', 'try',
  'type', 'typeof', 'unsafe', 'unsized', 'use', 'virtual', 'where', 'while', 'yield',
])
const MOTS_SANS_PREFIXE = new Set(['self', 'Self', 'super', 'crate'])

const mots = (s) => String(s).replace(/([a-z0-9])([A-Z])/g, '$1 $2').split(/[^A-Za-z0-9]+/).filter(Boolean)
const pascal = (s) => mots(s).map((m) => m[0].toUpperCase() + m.slice(1)).join('')
const serpent = (s) => mots(s).map((m) => m.toLowerCase()).join('_')
const chaine = (s) => JSON.stringify(String(s))

export function generer(doc) {
  const composants = doc.components ?? {}
  const declarations = []
  const noms = new Set()

  const echec = (message) => { throw new Error(message) }

  function reserver(nom) {
    if (!/^[A-Z][A-Za-z0-9]*$/.test(nom)) echec(`nom de type invalide : ${nom}`)
    if (noms.has(nom)) echec(`deux types porteraient le nom ${nom}`)
    noms.add(nom)
    declarations.push('')
    return declarations.length - 1
  }

  function cible(ref, section) {
    const prefixe = `#/components/${section}/`
    if (typeof ref !== 'string' || !ref.startsWith(prefixe)) echec(`référence non gérée : ${ref} (attendu ${prefixe}…)`)
    const cle = ref.slice(prefixe.length)
    const valeur = composants[section]?.[cle]
    if (valeur === undefined) echec(`référence introuvable : ${ref}`)
    return { cle, valeur }
  }

  function fusionner(schema) {
    if (!schema.allOf) return schema
    const fusion = { type: 'object', properties: {}, required: [] }
    for (const partie of schema.allOf) {
      const resolue = fusionner(partie.$ref ? cible(partie.$ref, 'schemas').valeur : partie)
      if (!resolue.properties) echec('allOf : chaque partie doit être un objet avec des propriétés')
      Object.assign(fusion.properties, resolue.properties)
      fusion.required.push(...(resolue.required ?? []))
    }
    Object.assign(fusion.properties, schema.properties ?? {})
    fusion.required.push(...(schema.required ?? []))
    return fusion
  }

  function defautable(schema, vus = []) {
    if (schema.$ref) return !vus.includes(schema.$ref) && defautable(cible(schema.$ref, 'schemas').valeur, [...vus, schema.$ref])
    if (schema.nullable) return true
    const s = fusionner(schema)
    if (s.type === 'string') return !s.enum
    if (s.type === 'object' && s.properties) return (s.required ?? []).every((cle) => !s.properties[cle] || defautable(s.properties[cle], vus))
    return true
  }

  function verifierCycles(schema, chemin) {
    const s = fusionner(schema.$ref ? cible(schema.$ref, 'schemas').valeur : schema)
    const suite = schema.$ref ? [...chemin, schema.$ref] : chemin
    if (schema.$ref && chemin.includes(schema.$ref)) echec(`schéma récursif non géré : ${suite.join(' → ')}`)
    if (s.type === 'object' && s.properties) for (const p of Object.values(s.properties)) verifierCycles(p, suite)
  }

  function declarerEnum(nom, valeurs) {
    const place = reserver(nom)
    const lignes = ['#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]', `pub enum ${nom} {`]
    const variantes = new Set(['Inconnu'])
    for (const valeur of valeurs) {
      const brut = valeur === '' ? 'Vide' : pascal(valeur)
      const variante = /^[0-9]/.test(brut) ? `V${brut}` : brut
      if (!variante || variantes.has(variante)) echec(`${nom} : la valeur ${chaine(valeur)} ne donne pas de variante distincte`)
      variantes.add(variante)
      lignes.push(`    #[serde(rename = ${chaine(valeur)})]`, `    ${variante},`)
    }
    lignes.push('    #[serde(other)]', '    Inconnu,', '}')
    declarations[place] = lignes.join('\n')
    return nom
  }

  function declarerStruct(nom, schema) {
    const place = reserver(nom)
    const requis = new Set(schema.required ?? [])
    const champs = Object.entries(schema.properties ?? {})
    const derives = ['Debug', 'Clone', ...(defautable(schema) ? ['Default'] : []), 'PartialEq', 'Serialize', 'Deserialize']
    const lignes = [`#[derive(${derives.join(', ')})]`]
    if (champs.length === 0) lignes.push(`pub struct ${nom} {}`)
    else {
      lignes.push(`pub struct ${nom} {`)
      const idents = new Set()
      for (const [cle, propriete] of champs) {
        const base = serpent(cle)
        if (!/^[a-z_][a-z0-9_]*$/.test(base) || idents.has(base)) echec(`${nom} : le champ ${chaine(cle)} ne donne pas d'identifiant distinct`)
        idents.add(base)
        const ident = MOTS_SANS_PREFIXE.has(base) ? `${base}_` : MOTS_RESERVES.has(base) ? `r#${base}` : base
        const interne = typeDe({ ...propriete, nullable: false }, nom + pascal(cle))
        const facultatif = !requis.has(cle)
        const type = facultatif || propriete.nullable ? `Option<${interne}>` : interne
        if (ident.replace(/^r#/, '') !== cle) lignes.push(`    #[serde(rename = ${chaine(cle)})]`)
        if (facultatif) lignes.push('    #[serde(skip_serializing_if = "Option::is_none")]')
        lignes.push(`    pub ${ident}: ${type},`)
      }
      lignes.push('}')
    }
    declarations[place] = lignes.join('\n')
    return nom
  }

  function typeDe(schema, nom) {
    if (schema === null || typeof schema !== 'object') return JSON_BRUT
    if (schema.nullable) return `Option<${typeDe({ ...schema, nullable: false }, nom)}>`
    if (schema.$ref) return pascal(cible(schema.$ref, 'schemas').cle)
    if (schema.allOf) return declarerStruct(nom, fusionner(schema))
    if (schema.oneOf || schema.anyOf) return JSON_BRUT
    switch (schema.type) {
      case 'string': return schema.enum?.every((v) => typeof v === 'string') ? declarerEnum(nom, schema.enum) : 'String'
      case 'integer': return 'i64'
      case 'number': return 'f64'
      case 'boolean': return 'bool'
      case 'array': return `Vec<${typeDe(schema.items ?? {}, `${nom}Element`)}>`
      case 'object':
        if (schema.properties) return declarerStruct(nom, schema)
        if (schema.additionalProperties && typeof schema.additionalProperties === 'object') return `BTreeMap<String, ${typeDe(schema.additionalProperties, `${nom}Valeur`)}>`
        return JSON_BRUT
      default: return JSON_BRUT
    }
  }

  function declarerAlias(nom, type) {
    const place = reserver(nom)
    const ligne = `pub type ${nom} = ${type};`
    declarations[place] = ligne.length <= LARGEUR ? ligne : `pub type ${nom} =\n    ${type};`
  }

  function nommer(nom, schema) {
    const type = typeDe(schema, nom)
    if (type !== nom) declarerAlias(nom, type)
  }

  const schemaJson = (conteneur) => conteneur?.content?.['application/json']?.schema

  for (const [cle, schema] of Object.entries(composants.schemas ?? {})) {
    verifierCycles({ $ref: `#/components/schemas/${cle}` }, [])
    nommer(pascal(cle), schema)
  }
  const nommes = { requestBodies: new Set(), responses: new Set() }
  for (const section of ['requestBodies', 'responses']) {
    for (const [cle, conteneur] of Object.entries(composants[section] ?? {})) {
      const schema = schemaJson(conteneur)
      if (!schema || schema.$ref) continue
      nommer(pascal(cle), schema)
      nommes[section].add(cle)
    }
  }

  function contenu(conteneur, section, nom) {
    if (!conteneur) return
    if (conteneur.$ref) {
      const { cle, valeur } = cible(conteneur.$ref, section)
      if (nommes[section].has(cle)) return declarerAlias(nom, pascal(cle))
      return contenu(valeur, section, nom)
    }
    const schema = schemaJson(conteneur)
    if (schema) nommer(nom, schema)
  }

  for (const [chemin, element] of Object.entries(doc.paths ?? {})) {
    for (const methode of METHODES) {
      const operation = element[methode]
      if (!operation) continue
      const base = pascal(methode) + pascal(chemin)
      contenu(operation.requestBody, 'requestBodies', `${base}Requete`)
      const reussites = Object.entries(operation.responses ?? {}).filter(([code, reponse]) => /^2\d\d$/.test(code) && (reponse.$ref || schemaJson(reponse)))
      for (const [code, reponse] of reussites) contenu(reponse, 'responses', reussites.length > 1 ? `${base}Reponse${code}` : `${base}Reponse`)
    }
  }

  const corps = declarations.join('\n\n')
  const texte = `${corps.includes('BTreeMap<') ? 'use std::collections::BTreeMap;\n\n' : ''}use serde::{Deserialize, Serialize};\n\n${corps}\n`
  const longue = texte.split('\n').find((ligne) => ligne.length > LARGEUR)
  if (longue) echec(`ligne de plus de ${LARGEUR} caractères, que rustfmt reformaterait : ${longue.trim()}`)
  return texte
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const options = process.argv.slice(2)
  if (options.some((o) => o !== '--check')) { console.error(`usage : ${COMMANDE} [--check]`); process.exit(2) }
  let attendu
  try {
    attendu = generer(parse(readFileSync(SOURCE, 'utf8')))
  } catch (e) {
    console.error(`génération impossible depuis docs/estompe/openapi.yaml : ${e.message}`)
    process.exit(1)
  }
  const sortie = fileURLToPath(SORTIE)
  if (options.includes('--check')) {
    let actuel = null
    try { actuel = readFileSync(sortie, 'utf8') } catch { actuel = null }
    if (actuel !== attendu) {
      console.error(`apps/estompe/crates/client/src/genere/types.rs ${actuel === null ? 'est absent' : 'ne correspond plus à docs/estompe/openapi.yaml'} : lancer « ${COMMANDE} » et commiter le résultat.`)
      process.exit(1)
    }
    console.log('types du client natif à jour avec docs/estompe/openapi.yaml')
  } else {
    writeFileSync(sortie, attendu)
    console.log(`écrit : ${sortie}`)
  }
}
