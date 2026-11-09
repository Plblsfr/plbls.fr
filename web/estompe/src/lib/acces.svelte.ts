import { untrack } from 'svelte'
import { api } from './api'
import { messageOf, notify } from './ui'

export const enc = encodeURIComponent
const ECHEC = 'aegis a refusé la demande'

export type Effet = 'allow' | 'deny'
export type Sujet = 'role' | 'group' | 'account'
export type Etat = 'autorise' | 'partiel' | 'refuse' | 'non_defini'

export interface Compte { sub: string; email: string | null; display_name: string | null; last_seen_at: string | null; in_keycloak: boolean }
export interface CompteResume extends Compte { roles_count: number; groups_count: number; allows_count: number; denies_count: number }
export interface PageComptes { accounts: CompteResume[]; total: number; from_keycloak: number }
export interface RegleVue { id: string; subject_kind: Sujet; subject_id: string; permission: string; resource: string; effect: Effet; expires_at: string | null; note: string; granted_by: string; created_at: string; obsolete: boolean }
export interface LiaisonVue { id: string; role_id: string; role: string; subject_kind: Sujet; subject_id: string; subject_label: string | null; scope_type: string | null; scope_id: string | null; expires_at: string | null; granted_by: string; created_at: string }
export interface Appartenance { group_id: string; name: string; expires_at: string | null; added_by: string; added_at: string }
export interface CompteDetail { account: Compte; bindings: LiaisonVue[]; groups: Appartenance[]; rules: RegleVue[]; group_bindings: LiaisonVue[] }
export type Source =
  | { kind: 'tout_le_monde'; role_id: string }
  | { kind: 'role'; role_id: string; role: string; binding_id: string; group_id: string | null; group: string | null; scope_type: string | null; scope_id: string | null }
  | { kind: 'groupe'; group_id: string; group: string }
  | { kind: 'compte' }
export interface Regle { rule_id: string; permission: string; resource: string; effect: Effet; expires_at: string | null; source: Source }
export interface Explication { permission: string; state: Etat; allow: string[]; deny: string[]; rules: Regle[] }
export interface EffectifsExpliques { account: Compte; permissions: Explication[] }
export interface Decision { allowed: boolean; reason: string; rules: Regle[] }
export interface PermissionVue { name: string; app: string; key: string; category: string; label: string; description: string; resource_type: string | null; open_default: boolean; obsolete_at: string | null; rules_count: number }
export interface TypeVue { app: string; key: string; name: string; listable: boolean }
export interface AppVue { key: string; name: string; description: string; seen_at: string; resource_types: TypeVue[]; permissions: PermissionVue[] }
export interface Catalogue { apps: AppVue[] }
export interface RoleResume { id: string; name: string; description: string; system: 'tout-le-monde' | 'admin' | null; created_at: string; rules_count: number; bindings_count: number; accounts_count: number }
export interface RoleDetail extends RoleResume { rules: RegleVue[]; bindings: LiaisonVue[] }
export interface GroupeResume { id: string; name: string; description: string; created_at: string; members_count: number; roles_count: number }
export interface Membre { account: Compte; expires_at: string | null; added_by: string; added_at: string }
export interface GroupeDetail extends GroupeResume { members: Membre[]; bindings: LiaisonVue[]; rules: RegleVue[] }
export interface EntreeJournal { id: number; at: string; actor: string; action: string; target_kind: string; target_id: string; target_label: string | null; subject: string | null; detail: Record<string, unknown>; actor_account: Compte | null; subject_account: Compte | null }
export interface Ressource { id: string; name: string }
export interface Detenteur { rule: RegleVue; subject_label: string | null; account: Compte | null }
export interface RegleSaisie { permission: string; resource: string; effect: Effet; expires_at?: string | null; note?: string }

export const lire = <T>(chemin: string) => api.acces<T>('GET', chemin)
export const ecrire = <T = void>(methode: string, chemin: string, corps?: unknown) => api.acces<T>(methode, chemin, corps)

export function executer<T>(p: Promise<T>, fait?: string): Promise<T | null> {
  return p.then((r) => { if (fait) notify.ok(fait); return r }, (e) => { notify.error(e, ECHEC); return null })
}

export interface Charge<T> { readonly data: T | null; readonly error: string | null; reload: () => void }

export function charger<T>(chemin: () => string | null): Charge<T> {
  let data = $state.raw<T | null>(null)
  let error = $state<string | null>(null)
  let courant: string | null = null
  const reload = () => {
    const c = courant
    if (!c) { data = null; return }
    error = null
    lire<T>(c).then(
      (d) => { if (courant === c) data = d },
      (e) => { if (courant === c) error = messageOf(e, ECHEC) },
    )
  }
  $effect(() => {
    const c = chemin()
    if (c === courant) return
    courant = c
    data = null
    untrack(reload)
  })
  return { get data() { return data }, get error() { return error }, reload }
}

export const nomCompte = (c: Pick<Compte, 'email' | 'display_name' | 'sub'> | null | undefined, sub = '') =>
  c?.email ?? c?.display_name ?? (c?.sub || sub)

export const quand = (d: string | null | undefined) => (d ? new Date(d).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' }) : '—')

export const versIso = (local: string) => (local ? new Date(local).toISOString() : null)

export const ETATS: Record<Etat, { label: string; ton: string }> = {
  autorise: { label: 'Autorisé', ton: 'ok' },
  partiel: { label: 'Partiel', ton: 'info' },
  refuse: { label: 'Refusé', ton: 'dgr' },
  non_defini: { label: 'Non défini', ton: '' },
}

export const origine = (s: Source): string => {
  switch (s.kind) {
    case 'tout_le_monde': return 'Tout le monde'
    case 'compte': return 'règle directe'
    case 'groupe': return `groupe ${s.group}`
    case 'role': return `rôle ${s.role}${s.group ? ` (via le groupe ${s.group})` : ''}${s.scope_id ? ` sur ${s.scope_id}` : ''}`
  }
}

export const ACTIONS: Record<string, string> = {
  manifest: 'manifeste reçu', 'role.create': 'rôle créé', 'role.update': 'rôle modifié', 'role.delete': 'rôle supprimé',
  'group.create': 'groupe créé', 'group.update': 'groupe modifié', 'group.delete': 'groupe supprimé',
  'group.member.add': 'ajouté au groupe', 'group.member.remove': 'retiré du groupe',
  'binding.create': 'rôle donné', 'binding.delete': 'rôle retiré', 'rule.allow': 'permission accordée', 'rule.deny': 'permission refusée',
  'rule.update': 'règle modifiée', 'rule.delete': 'règle supprimée', 'account.copy': 'droits copiés', 'account.purge': 'droits retirés',
  'permissions.purge': 'permissions obsolètes purgées', expire: 'expiré',
}

export const tonAction = (a: string) =>
  a.endsWith('.deny') || a.endsWith('.delete') || a.endsWith('.remove') || a.endsWith('.purge') || a === 'expire' ? 'dgr' : a.endsWith('.update') ? 'info' : 'ok'

export const libellesPermissions = (c: Catalogue | null): Map<string, PermissionVue> =>
  new Map((c?.apps ?? []).flatMap((a) => a.permissions.map((p) => [p.name, p] as const)))

const nomsConnus = new Map<string, Promise<string>>()

export function nomRessource(type: string | null | undefined, id: string): Promise<string> {
  if (!type || id === '*') return Promise.resolve(id === '*' ? 'partout' : id)
  const cle = `${type}|${id}`
  let p = nomsConnus.get(cle)
  if (!p) {
    p = lire<Ressource[]>(`/resources/${type.split(':').map(enc).join('/')}?ids=${enc(id)}`).then(
      (l) => (Array.isArray(l) ? l.find((r) => r.id === id)?.name : undefined) ?? id,
      () => { nomsConnus.delete(cle); return id },
    )
    nomsConnus.set(cle, p)
  }
  return p
}

export const typeDe = (noms: Map<string, PermissionVue>, permission: string): string | null => {
  const p = noms.get(permission)
  return p?.resource_type ? `${p.app}:${p.resource_type}` : null
}
