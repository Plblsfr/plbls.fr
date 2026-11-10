/** Pont Kotlin de l'app Android (MainActivity) : jeton FCM, redirection de connexion, installation de l'APK, mode conduite ; session et capture par le canal EstompeSession. */
export interface Bridge {
  fcmToken(): string; takeAuthUrl(): string; installApk(url: string, sha256: string): void; installState(): string; installProgress(): number
  takeOpenUrl(): string; cancelConv(conv: string): void
  bgAudioUpdate?(json: string): void; bgAudioStop?(): void
  voiceUpdate(title: string, href: string, muted: boolean, deafened: boolean): void; voiceStop(): void
  // mode conduite (DriveService)
  driveSettings(): string; saveDriveSettings(json: string): void; driveActive(): boolean; driveStart(): string; driveStop(): void
  pipState?(json: string): void
  driveEngines(): string; driveBluetooth(): string; driveModel(): string; driveDownloadModel(): void; driveLog(): string
}

export const nativeBridge = () => (window as unknown as { EstompeNative?: Bridge }).EstompeNative

export type SessionRequest = { op: 'login'; code: string; verifier: string; redirectUri: string } | { op: 'renew' } | { op: 'logout' } | { op: 'adopt'; refresh: string } | { op: 'screenshot' }
export interface SessionReply { status: number; access_token?: string; expires_in?: number; error?: string; image?: string }
export type SessionCall = (r: SessionRequest) => Promise<SessionReply>
interface SessionChannel { postMessage(m: string): void; onmessage: ((e: { data: string }) => void) | null }

let channel: SessionChannel | undefined
let nextId = 0
const waiting = new Map<number, (r: SessionReply) => void>()

export function nativeSession(): SessionCall | undefined {
  const c = (window as unknown as { EstompeSession?: SessionChannel }).EstompeSession
  if (!c) return undefined
  if (c !== channel) {
    channel = c
    c.onmessage = (e) => {
      const r = JSON.parse(e.data) as SessionReply & { id: number }
      waiting.get(r.id)?.(r)
      waiting.delete(r.id)
    }
  }
  return (r) => new Promise((resolve) => {
    const id = ++nextId
    waiting.set(id, resolve)
    c.postMessage(JSON.stringify({ ...r, id }))
  })
}
