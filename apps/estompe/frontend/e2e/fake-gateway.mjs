// Faux auth-gateway pour les tests : GET /auth/me répond avec l'identité contenue dans le cookie ou le Bearer
// (la valeur sert d'id et de nom), et faux permission.plbls.fr : POST /v1/check autorise tout le monde sauf « mallory ».
// Jamais utilisé hors tests.
import http from 'node:http'

const port = Number(process.env.PORT ?? 19000)
http
  .createServer((req, res) => {
    if (req.method === 'POST' && req.url === '/v1/check') {
      let body = ''
      req.on('data', (c) => (body += c))
      req.on('end', () => {
        const { user_id } = JSON.parse(body)
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ allowed: user_id !== 'kc-mallory' }))
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
