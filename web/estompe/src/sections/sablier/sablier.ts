import { requestAt } from '../../lib/api'
import { config } from '../../lib/config'

export const SABLIER_URL = config.sablierUrl
export const sablierOn = () => !!SABLIER_URL

export const PLATEFORMES = ['web', 'pwa', 'windows', 'android'] as const
export type Plateforme = (typeof PLATEFORMES)[number]

export const SECTIONS = ['messagerie', 'entracte', 'sillon', 'regie', 'sablier', 'admin', 'autre'] as const
export type Section = (typeof SECTIONS)[number]

export interface SablierMe {
  sub: string
  name: string
  email: string
  permissions: { access: boolean; admin: boolean }
  sections: string[]
  heartbeat_seconds?: number
}

const appel = <T>(methode: string, chemin: string, corps?: unknown) => requestAt<T>(SABLIER_URL, methode, chemin, corps, { discret: true })

export const sablierApi = {
  me: () => appel<SablierMe>('GET', '/me'),
  battre: (platform: Plateforme, section: Section) => appel<void>('POST', '/heartbeat', { platform, section }),
}
