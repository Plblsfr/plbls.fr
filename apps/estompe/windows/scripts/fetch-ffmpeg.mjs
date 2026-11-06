// Télécharge ffmpeg (build LGPL de BtbN, sans libx264 : l'export réencode avec les encodeurs matériels) dans windows/src-tauri/binaries/,
// d'où tauri.windows.conf.json l'embarque dans l'installeur. Sans effet hors build Windows ou si le fichier est déjà là.
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, copyFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

// Version épinglée : pour la changer, mettre à jour TAG, ARCHIVE et SHA256 (sha256sum de l'archive).
const TAG = 'autobuild-2026-10-26-14-25'
const ARCHIVE = 'ffmpeg-N-128100-gceb466cbcd-win64-lgpl.zip'
const SHA256 = 'a46195b14ea18385570a9b154868813495195f199b9c886d67b16d474c883b34'
const URL = `https://github.com/BtbN/FFmpeg-Builds/releases/download/${TAG}/${ARCHIVE}`
const dest = join(import.meta.dirname, '..', 'src-tauri', 'binaries', 'ffmpeg.exe')

if (process.env.TAURI_ENV_PLATFORM !== 'windows' || existsSync(dest)) process.exit(0)
const tmp = mkdtempSync(join(tmpdir(), 'ffmpeg-'))
try {
  const zip = join(tmp, 'ffmpeg.zip')
  const res = await fetch(URL)
  if (!res.ok) throw new Error(`téléchargement de ffmpeg : HTTP ${res.status}`)
  const data = Buffer.from(await res.arrayBuffer())
  const sha = createHash('sha256').update(data).digest('hex')
  if (sha !== SHA256) throw new Error(`somme SHA-256 de ffmpeg inattendue : ${sha} (attendue ${SHA256})`)
  writeFileSync(zip, data)
  execFileSync('unzip', ['-q', '-j', zip, '*/bin/ffmpeg.exe', '-d', tmp])
  mkdirSync(join(dest, '..'), { recursive: true })
  copyFileSync(join(tmp, 'ffmpeg.exe'), dest)
} finally {
  rmSync(tmp, { recursive: true, force: true })
}
