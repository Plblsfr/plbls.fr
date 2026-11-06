import { describe, expect, it, vi } from 'vitest'
import { MicProcessor, rmsDb } from './micprocessor'

describe('rmsDb', () => {
  it('silence', () => {
    expect(rmsDb([])).toBe(-100)
    expect(rmsDb(new Float32Array(512))).toBe(-100)
  })
  it('pleine échelle ≈ -3 dBFS pour une sinusoïde', () => {
    const s = Array.from({ length: 4800 }, (_, i) => Math.sin((2 * Math.PI * 440 * i) / 48000))
    expect(rmsDb(s)).toBeCloseTo(-3.01, 1)
  })
  it('une amplitude 10× plus faible vaut 20 dB de moins', () => {
    const loud = Array.from({ length: 4800 }, (_, i) => 0.5 * Math.sin((2 * Math.PI * 440 * i) / 48000))
    const soft = loud.map((x) => x / 10)
    expect(rmsDb(loud) - rmsDb(soft)).toBeCloseTo(20, 1)
  })
})

describe('MicProcessor', () => {
  it('publie une piste mono : chaque nœud est forcé à un canal', async () => {
    const made: AudioNode[] = []
    const gains: AudioNode[] = []
    const node = () => {
      const n = { connect: (x: AudioNode) => x, disconnect() {}, gain: { value: 1, setTargetAtTime() {} }, fftSize: 0 } as unknown as AudioNode
      made.push(n)
      return n
    }
    const ctx = {
      createMediaStreamSource: node,
      createGain: () => {
        const g = node()
        gains.push(g)
        return g
      },
      createAnalyser: node,
      createMediaStreamDestination: () => Object.assign(node(), { stream: { getAudioTracks: () => [{}] } }),
    }
    vi.stubGlobal('MediaStream', class {})
    await new MicProcessor(false).init({ audioContext: ctx, track: {} } as never)
    const dest = made.find((n) => 'stream' in n)!
    expect([dest.channelCount, dest.channelCountMode]).toEqual([1, 'explicit'])
    expect(gains.length).toBe(2)
    expect(gains.every((n) => n.channelCount === 1 && n.channelCountMode === 'explicit')).toBe(true)
  })
})
