import { describe, expect, it } from 'vitest'
import { youtubeVideo } from './youtube'

describe('youtubeVideo', () => {
  it('reconnaît les formats youtube', () => {
    const id = 'dQw4w9WgXcQ'
    for (const u of [`https://www.youtube.com/watch?v=${id}`, `https://youtu.be/${id}`, `https://youtube.com/shorts/${id}`, `https://m.youtube.com/embed/${id}`])
      expect(youtubeVideo(u)).toEqual({ id, start: 0 })
  })
  it('lit le temps de départ', () => {
    expect(youtubeVideo('https://youtu.be/dQw4w9WgXcQ?t=90')?.start).toBe(90)
    expect(youtubeVideo('https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1h2m3s')?.start).toBe(3723)
  })
  it('ignore le reste', () => {
    for (const u of ['https://example.com/watch?v=dQw4w9WgXcQ', 'https://youtu.be/court', 'https://www.youtube.com/', 'pas un lien'])
      expect(youtubeVideo(u)).toBeNull()
  })
})
