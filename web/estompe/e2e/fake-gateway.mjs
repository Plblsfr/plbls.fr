// Faux auth-gateway pour les tests : GET /auth/me répond avec l'identité contenue dans le cookie ou le Bearer
// (la valeur sert d'id et de nom), et faux aegis : PUT /v1/manifest enregistre le catalogue d'Estompe, POST /v1/effective
// accorde toutes ses permissions à tout le monde sauf « mallory », qui ne reçoit rien.
// Jamais utilisé hors tests.
import { readFileSync } from 'node:fs'
import http from 'node:http'

const port = Number(process.env.PORT ?? 19000)
const catalogue = new URL('../../../api/estompe/core/src/catalogue.rs', import.meta.url)
const cles = new Set([...readFileSync(catalogue, 'utf8').matchAll(/\be\("([a-z][a-z.]*)"/g)].map((m) => m[1]))

const lire = (req) =>
  new Promise((ok) => {
    let body = ''
    req.on('data', (c) => (body += c))
    req.on('end', () => ok(body ? JSON.parse(body) : {}))
  })
const json = (res, corps) => {
  res.writeHead(200, { 'Content-Type': 'application/json' })
  res.end(JSON.stringify(corps))
}

http
  .createServer(async (req, res) => {
    if (req.method === 'PUT' && req.url === '/v1/manifest') {
      const m = await lire(req)
      for (const p of m.permissions ?? []) cles.add(p.key)
      json(res, {})
      return
    }
    if (req.method === 'POST' && req.url === '/v1/effective') {
      const { sub } = await lire(req)
      const permissions = sub === 'kc-mallory' ? {} : Object.fromEntries([...cles].map((c) => [`estompe:${c}`, { allow: ['*'], deny: [] }]))
      json(res, { sub, permissions })
      return
    }
    if (req.url !== '/auth/me') { res.writeHead(404).end(); return }
    const cookie = /(?:^|;\s*)__Secure-plbls_sid=([^;]+)/.exec(req.headers.cookie ?? '')?.[1]
    const bearer = /^Bearer (.+)$/.exec(req.headers.authorization ?? '')?.[1]
    const who = cookie ?? bearer
    if (!who) { res.writeHead(401).end(); return }
    res.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
    res.end(JSON.stringify({ id: `kc-${who}`, name: who, email: '', roles: [], client: 'test' }))
  })
  .listen(port, '127.0.0.1')
