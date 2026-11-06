// Extrait l'identifiant (et le temps de départ en secondes) d'un lien YouTube : watch, youtu.be, shorts, embed.
export function youtubeVideo(url: string): { id: string; start: number } | null {
  let u: URL
  try { u = new URL(url) } catch { return null }
  const host = u.hostname.replace(/^(www|m)\./, '')
  let id: string | null = null
  if (host === 'youtu.be') id = u.pathname.slice(1).split('/')[0]
  else if (host === 'youtube.com') {
    const m = u.pathname.match(/^\/(?:shorts|embed)\/([^/]+)/)
    id = m ? m[1] : u.pathname === '/watch' ? u.searchParams.get('v') : null
  }
  if (!id || !/^[\w-]{11}$/.test(id)) return null
  const t = u.searchParams.get('t') ?? u.searchParams.get('start') ?? ''
  const h = t.match(/^(?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s?)?$/)
  const start = h ? Number(h[1] ?? 0) * 3600 + Number(h[2] ?? 0) * 60 + Number(h[3] ?? 0) : 0
  return { id, start }
}
