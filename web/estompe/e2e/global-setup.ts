import { execFileSync, spawn, type ChildProcess } from 'node:child_process'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

export const E2E = {
  api: 'http://localhost:18080',
  app: 'http://localhost:4173',
}

const root = resolve(import.meta.dirname, '..', '..', '..')
const procs: ChildProcess[] = []
let tmp = ''

async function waitFor(url: string, ms = 60_000) {
  const end = Date.now() + ms
  while (Date.now() < end) {
    try { if ((await fetch(url)).ok) return } catch { /* pas encore prêt */ }
    await new Promise((r) => setTimeout(r, 300))
  }
  throw new Error(`${url} ne répond pas`)
}

/** Démarre PostgreSQL (Docker), le faux gateway (et faux aegis), le vrai backend et le build du front. */
export default async function setup() {
  tmp = mkdtempSync(join(tmpdir(), 'estompe-e2e-'))
  execFileSync('cargo', ['build', '-p', 'estompe-api'], { cwd: root, stdio: 'inherit' })
  const bin = join(resolve(root, process.env.CARGO_TARGET_DIR ?? 'target'), 'debug/estompe-api')

  execFileSync('docker', ['rm', '-f', 'estompe-e2e-pg'], { stdio: 'ignore' })
  execFileSync('docker', ['run', '-d', '--rm', '--name', 'estompe-e2e-pg', '-p', '55432:5432', '-e', 'POSTGRES_PASSWORD=p', '-e', 'POSTGRES_DB=estompe', 'postgres:17-alpine'], { stdio: 'ignore' })

  // Vrai serveur LiveKit (réseau de l'hôte : le navigateur de test joint 127.0.0.1) ; les webhooks vont vers le vrai backend.
  const lkConfig = `port: 7880
rtc:
  tcp_port: 7881
  port_range_start: 50000
  port_range_end: 50100
  node_ip: 127.0.0.1
  use_external_ip: false
keys:
  k: ${'x'.repeat(32)}
webhook:
  api_key: k
  urls:
    - http://127.0.0.1:18080/voice/webhook
`
  execFileSync('docker', ['rm', '-f', 'estompe-e2e-lk'], { stdio: 'ignore' })
  execFileSync('docker', ['run', '-d', '--rm', '--name', 'estompe-e2e-lk', '--network', 'host', '-e', `LIVEKIT_CONFIG=${lkConfig}`, 'livekit/livekit-server:latest'], { stdio: 'ignore' })

  procs.push(spawn('node', [join(import.meta.dirname, 'fake-gateway.mjs')], { stdio: 'inherit' }))
  procs.push(
    spawn(bin, [], {
      stdio: 'inherit',
      env: {
        ...process.env,
        LISTEN_ADDR: '127.0.0.1:18080',
        DATABASE_URL: 'postgres://postgres:p@127.0.0.1:55432/estompe?sslmode=disable',
        GATEWAY_URL: 'http://127.0.0.1:19000',
        ALLOWED_ORIGINS: E2E.app,
        AEGIS_URL: 'http://127.0.0.1:19000',
        AEGIS_TOKEN: 'jeton-de-test',
        DATA_DIR: join(tmp, 'data'),
        LIVEKIT_URL: 'ws://127.0.0.1:7880',
        LIVEKIT_API_URL: 'http://127.0.0.1:7880',
        LIVEKIT_API_KEY: 'k',
        LIVEKIT_API_SECRET: 'x'.repeat(32),
        LIVEKIT_ALLOW_VIDEO: 'true',
      },
    }),
  )
  await waitFor(`${E2E.api}/readyz`, 90_000)
  return async () => {
    procs.forEach((p) => p.kill())
    execFileSync('docker', ['rm', '-f', 'estompe-e2e-pg', 'estompe-e2e-lk'], { stdio: 'ignore' })
    rmSync(tmp, { recursive: true, force: true })
  }
}
