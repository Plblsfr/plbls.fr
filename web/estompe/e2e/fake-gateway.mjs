// Faux auth-gateway pour les tests : GET /auth/me répond avec l'identité contenue dans le cookie ou le Bearer
// (la valeur sert d'id et de nom), et faux aegis : POST /v1/effective accorde toutes les permissions du catalogue
// à tout le monde sauf « mallory », PUT /v1/manifest accepte le manifeste. Jamais utilisé hors tests.
import { readFileSync } from 'node:fs'
import http from 'node:http'
import { join } from 'node:path'

const port = Number(process.env.PORT ?? 19000)
const catalogue = readFileSync(join(import.meta.dirname, '../../../api/estompe/core/src/catalogue.rs'), 'utf8')
const toutes = Object.fromEntries(
  [...catalogue.matchAll(/^\s*e\(\s*"([a-z0-9._-]+)"/gm)].map((m) => [`estompe:${m[1]}`, { allow: ['*'], deny: [] }]),
)
http
  .createServer((req, res) => {
    if (req.method === 'POST' && req.url === '/v1/effective') {
      let body = ''
      req.on('data', (c) => (body += c))
      req.on('end', () => {
        const { sub } = JSON.parse(body)
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ sub, permissions: sub === 'kc-mallory' ? {} : toutes }))
      })
      return
    }
    if (req.method === 'PUT' && req.url === '/v1/manifest') {
      req.resume()
      req.on('end', () => {
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ changed: false }))
      })
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
