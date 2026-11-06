/** Pont Kotlin de l'app Android (MainActivity) : jeton FCM, redirection de connexion, installation de l'APK, mode conduite. */
export interface Bridge {
  fcmToken(): string; takeAuthUrl(): string; installApk(url: string, sha256: string): void; installState(): string; installProgress(): number
  getRefreshToken(): string; saveRefreshToken(t: string): void; clearRefreshToken(): void; configure(issuer: string, apiUrl: string): void
  takeOpenUrl(): string; cancelConv(conv: string): void; screenshot(): string
  bgAudioUpdate?(json: string): void; bgAudioStop?(): void
  voiceUpdate(title: string, href: string, muted: boolean, deafened: boolean): void; voiceStop(): void
  // mode conduite (DriveService)
  driveSettings(): string; saveDriveSettings(json: string): void; driveActive(): boolean; driveStart(): string; driveStop(): void
  pipState?(json: string): void
  driveEngines(): string; driveBluetooth(): string; driveModel(): string; driveDownloadModel(): void; driveLog(): string
}

export const nativeBridge = () => (window as unknown as { EstompeNative?: Bridge }).EstompeNative
