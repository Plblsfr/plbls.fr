export type Reseau = 'wifi' | 'cellular' | 'unknown'
export interface NetworkDriver {
  kind(): Reseau
  watch(onChange: () => void): () => void
}

interface Connexion { type?: string; addEventListener?(e: 'change', f: () => void): void; removeEventListener?(e: 'change', f: () => void): void }
const connexion = (): Connexion | undefined => (navigator as Navigator & { connection?: Connexion }).connection

export const reseauDuNavigateur: NetworkDriver = {
  kind() {
    const type = connexion()?.type
    if (type === 'wifi' || type === 'ethernet') return 'wifi'
    return type === 'cellular' ? 'cellular' : 'unknown'
  },
  watch(onChange) {
    const c = connexion()
    c?.addEventListener?.('change', onChange)
    return () => c?.removeEventListener?.('change', onChange)
  },
}
