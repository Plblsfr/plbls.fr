import { describe, expect, it } from 'vitest'
import { codecOfMoov, mp4Codec } from './video'

const box = (type: string, ...parts: Uint8Array[]) => {
  const body = parts.flatMap((p) => [...p])
  const out = new Uint8Array(8 + body.length)
  new DataView(out.buffer).setUint32(0, out.length)
  out.set([...type].map((c) => c.charCodeAt(0)), 4)
  out.set(body, 8)
  return out
}
const zeros = (n: number) => new Uint8Array(n)
const trak = (handler: string, entry: string) => box('trak', box('mdia', box('hdlr', zeros(8), new Uint8Array([...handler].map((c) => c.charCodeAt(0)))), box('minf', box('stbl', box('stsd', zeros(8), box(entry, zeros(8)))))))
const moov = (...t: Uint8Array[]) => box('moov', ...t)

describe('codecOfMoov', () => {
  it('lit le type de l’échantillon de la piste vidéo, pas celui de l’audio', () => {
    expect(codecOfMoov(moov(trak('soun', 'mp4a'), trak('vide', 'hvc1')))).toBe('hvc1')
    expect(codecOfMoov(moov(trak('vide', 'avc1'), trak('soun', 'mp4a')))).toBe('avc1')
  })
  it('renvoie null sans piste vidéo', () => expect(codecOfMoov(moov(trak('soun', 'mp4a')))).toBeNull())
})

describe('mp4Codec', () => {
  const ftyp = box('ftyp', new Uint8Array([105, 115, 111, 109]), zeros(4))
  it('trouve le moov en tête comme en fin de fichier', async () => {
    const m = moov(trak('vide', 'av01'))
    expect(await mp4Codec(new Blob([ftyp, m, box('mdat', zeros(100))]))).toBe('av01')
    expect(await mp4Codec(new Blob([ftyp, box('mdat', zeros(100)), m]))).toBe('av01')
  })
  it('renvoie null pour un fichier sans moov ou tronqué', async () => {
    expect(await mp4Codec(new Blob([ftyp, box('mdat', zeros(10))]))).toBeNull()
    expect(await mp4Codec(new Blob([zeros(3)]))).toBeNull()
  })
})
