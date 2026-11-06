// Génère frontend/src/blocks/emojis/emoji-data.json et backend/core/src/emoji/codes.json à partir d'emojibase-data.
// Usage : node scripts/estompe/gen-emojis.mjs <dossier de emojibase-data>   (données ponctuelles, pas une dépendance du projet)
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const dir = process.argv[2]
if (!dir) throw new Error('usage : node scripts/estompe/gen-emojis.mjs <emojibase-data>')
const load = (p) => JSON.parse(readFileSync(join(dir, p), 'utf8'))
const en = load('en/data.json')
const fr = new Map(load('fr/data.json').map((e) => [e.hexcode, e]))
const sources = ['github', 'iamcal', 'emojibase'].map((s) => load(`en/shortcodes/${s}.json`))
const GROUPS = ['smileys-emotion', 'people-body', 'animals-nature', 'food-drink', 'activities', 'travel-places', 'objects', 'symbols', 'flags']
const groupKeys = load('en/messages.json').groups.map((g) => g.key)
const MAX_VERSION = 15
const norm = (s) => s.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '')

const rows = []
const codes = {}
for (const e of en.filter((e) => e.group !== undefined && e.version <= MAX_VERSION).sort((a, b) => a.order - b.order)) {
  const g = GROUPS.indexOf(groupKeys[e.group])
  if (g < 0) continue
  const f = fr.get(e.hexcode)
  const found = sources.flatMap((s) => [s[e.hexcode] ?? []].flat())
  const slug = e.label.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_|_$/g, '')
  const names = [...new Set([...found, slug].map((c) => c.toLowerCase().replace(/[^a-z0-9_+-]/g, '_')))].filter((c) => /^[a-z0-9_+-]{2,}$/.test(c))
  const words = [...new Set([e.label, ...(e.tags ?? []), f?.label, ...(f?.tags ?? [])].filter(Boolean).flatMap((w) => norm(w).split(/[\s:,]+/)).filter((w) => w.length > 1))]
  rows.push([e.emoji, names[0], names.slice(1), words.join(' '), f?.label ?? e.label, g])
  for (const n of names) codes[n] ??= e.emoji
}
writeFileSync(new URL('../../web/estompe/src/blocks/emojis/emoji-data.json', import.meta.url), JSON.stringify(rows))
writeFileSync(new URL('../../api/estompe/core/src/emoji/codes.json', import.meta.url), JSON.stringify(codes))
console.log(rows.length, 'smileys,', Object.keys(codes).length, 'codes')
