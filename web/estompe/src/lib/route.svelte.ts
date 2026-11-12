/** Routeur minimal sur le hash : #/ · #/s/<serveur>/<salon>[/<message>] · #/dm/<id>[/<message>] · #/join/<code> · #/admin/<section> · #/<espace de la plateforme>/<section>  */
import { platform, platformReady } from './platform'


export type Route =
  | { name: 'home' }
  | { name: 'channel'; serverId: string; channelId?: string; messageId?: string }
  | { name: 'dm'; id: string; messageId?: string }
  | { name: 'join'; code: string }
  | { name: 'admin'; section?: string }
  | { name: 'films'; section?: string; id?: string }
  | { name: 'space'; space: string; section?: string }

export function parseHash(hash: string): Route {
  const parts = hash.replace(/^#\/?/, '').split('/').filter(Boolean).map(decodeURIComponent)
  if (parts[0] === 's' && parts[1]) return { name: 'channel', serverId: parts[1], channelId: parts[2], messageId: parts[3] }
  if (parts[0] === 'dm' && parts[1]) return { name: 'dm', id: parts[1], messageId: parts[2] }
  if (parts[0] === 'join' && parts[1]) return { name: 'join', code: parts[1] }
  if (parts[0] === 'admin') return { name: 'admin', section: parts[1] }
  if (parts[0] === 'films') return { name: 'films', section: parts[1], id: parts[2] }
  if (parts[0] && platformReady() && platform.spaces.some((s) => s.id === parts[0])) return { name: 'space', space: parts[0], section: parts[1] }
  return { name: 'home' }
}

export const hrefs = {
  home: () => '#/',
  channel: (serverId: string, channelId?: string) => `#/s/${serverId}${channelId ? '/' + channelId : ''}`,
  dm: (id: string) => `#/dm/${id}`,
  admin: (section?: string) => `#/admin${section ? '/' + section : ''}`,
  films: (section?: string, id?: string) => `#/films${section ? '/' + section : ''}${id ? '/' + encodeURIComponent(id) : ''}`,
  space: (space: string, section?: string) => `#/${space}${section ? '/' + section : ''}`,
  join: (code: string) => `#/join/${encodeURIComponent(code)}`,
}

class Router {
  current = $state<Route>(parseHash(typeof location === 'undefined' ? '' : location.hash))
  constructor() {
    if (typeof window !== 'undefined') window.addEventListener('hashchange', () => (this.current = parseHash(location.hash)))
  }
  go(href: string) {
    if (location.hash === href) this.current = parseHash(href)
    else location.hash = href
  }
}

export const router = new Router()
