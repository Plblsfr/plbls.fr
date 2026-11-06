import type { Member, PermKey, Role, ServerDetail } from './types'

export interface PermInfo { key: PermKey; label: string; desc: string }
export interface PermGroup { title: string; perms: PermInfo[] }

/** Permissions regroupées par catégorie, dans l'ordre d'affichage de l'éditeur de rôle. */
export const PERM_GROUPS: PermGroup[] = [
  { title: 'Général', perms: [
    { key: 'manage_server', label: 'Gérer le serveur', desc: 'Changer le nom et l\'icône du serveur.' },
    { key: 'manage_roles', label: 'Gérer les rôles', desc: 'Créer, modifier, ordonner et attribuer les rôles situés sous le sien.' },
    { key: 'manage_channels', label: 'Gérer les salons', desc: 'Créer, renommer, supprimer les salons et régler leurs permissions.' },
    { key: 'create_invites', label: 'Créer des invitations', desc: 'Créer et révoquer les liens d\'invitation.' },
    { key: 'view_audit_log', label: 'Voir le journal d\'administration', desc: 'Consulter le journal de modération et l\'historique des membres.' },
  ] },
  { title: 'Membres', perms: [
    { key: 'kick_members', label: 'Expulser des membres', desc: 'Retirer un membre du serveur ; il peut revenir avec une invitation.' },
    { key: 'ban_members', label: 'Bannir des membres', desc: 'Interdire le retour d\'un membre, définitivement ou pour une durée.' },
    { key: 'manage_nicknames', label: 'Changer le pseudo des autres', desc: 'Modifier le pseudo d\'un membre sur ce serveur.' },
    { key: 'timeout_members', label: 'Exclure temporairement des membres', desc: 'Bloquer les actions d\'un membre pendant une durée, ou l\'avertir.' },
  ] },
  { title: 'Salons texte', perms: [
    { key: 'view_channel', label: 'Voir le salon', desc: 'Voir le salon et lire ses messages.' },
    { key: 'send_messages', label: 'Envoyer des messages', desc: 'Écrire dans les salons texte.' },
    { key: 'attach_files', label: 'Envoyer des pièces jointes et des messages vocaux', desc: 'Joindre des fichiers et enregistrer des messages vocaux.' },
    { key: 'add_reactions', label: 'Ajouter des réactions', desc: 'Réagir aux messages avec un emoji.' },
    { key: 'mention_everyone', label: 'Mentionner @everyone et les rôles', desc: 'Notifier tout le salon ou les membres d\'un rôle.' },
    { key: 'manage_messages', label: 'Gérer les messages', desc: 'Supprimer et épingler les messages des autres, clore leurs sondages.' },
    { key: 'create_polls', label: 'Créer des sondages', desc: 'Lancer un sondage dans un salon.' },
  ] },
  { title: 'Salons vocaux', perms: [
    { key: 'connect', label: 'Rejoindre', desc: 'Se connecter aux salons vocaux.' },
    { key: 'speak', label: 'Parler', desc: 'Utiliser le micro.' },
    { key: 'video', label: 'Vidéo', desc: 'Allumer la caméra.' },
    { key: 'share_screen', label: 'Partager l\'écran', desc: 'Partager son écran.' },
    { key: 'mute_members', label: 'Couper le micro des membres', desc: 'Couper ou rétablir le micro d\'un participant.' },
    { key: 'deafen_members', label: 'Couper le son des membres', desc: 'Couper ou rétablir le son d\'un participant.' },
    { key: 'move_members', label: 'Déplacer des membres', desc: 'Changer un participant de salon vocal.' },
    { key: 'disconnect_members', label: 'Déconnecter des membres', desc: 'Retirer un participant du vocal sans l\'expulser du serveur.' },
  ] },
  { title: 'Esquisses', perms: [
    { key: 'organize_watch', label: 'Organiser des séances', desc: 'Planifier une séance « Regarder ensemble » ou d\'écoute et marquer les vidéos et morceaux comme vus ou écoutés. Publier et gérer les publications des autres suivent les permissions des salons texte.' },
  ] },
]

/** Permissions que l'on peut surcharger sur un salon (celles des salons texte et vocaux). */
export const CHANNEL_PERMS: PermKey[] = PERM_GROUPS.slice(3).flatMap((g) => g.perms.map((p) => p.key))

export const permLabel = (key: PermKey) => PERM_GROUPS.flatMap((g) => g.perms).find((p) => p.key === key)?.label ?? key

/** Rôles d'un membre, du plus élevé au plus bas (« @tout le monde » exclu). */
export function rolesOf(detail: ServerDetail | undefined, member: Pick<Member, 'roles'> | undefined): Role[] {
  const byId = new Map((detail?.roles ?? []).map((r) => [r.id, r]))
  return (member?.roles ?? []).map((id) => byId.get(id)).filter((r): r is Role => !!r).sort((a, b) => b.position - a.position)
}

/** Couleur du pseudo : celle du rôle le plus élevé qui en a une (undefined sans couleur). */
export function nameColor(detail: ServerDetail | undefined, member: Pick<Member, 'roles'> | undefined): string | undefined {
  return rolesOf(detail, member).find((r) => r.color)?.color
}

/** Position du rôle le plus élevé d'un membre (0 : seulement « @tout le monde »). */
export const topPosition = (detail: ServerDetail | undefined, member: Pick<Member, 'roles'> | undefined) => rolesOf(detail, member)[0]?.position ?? 0

/** Le membre est exclu temporairement à l'instant `now` (ms). */
export const isTimedOut = (m: Pick<Member, 'timeout_until'> | undefined, now = Date.now()) => !!m?.timeout_until && Date.parse(m.timeout_until) > now

/** Pseudo affiché sur le serveur : le pseudo de serveur s'il existe. */
export const shownName = (m: Pick<Member, 'pseudo' | 'nickname'>) => m.nickname || m.pseudo

export interface MemberGroup { role: Role | null; members: Member[] }

/**
 * Liste des membres groupée : un groupe par rôle « affiché séparément », du plus élevé au plus bas, chaque membre dans le groupe de son
 * rôle affiché le plus élevé ; les autres vont dans le groupe sans rôle (null), en dernier.
 */
export function groupMembers(detail: ServerDetail, members: Member[]): MemberGroup[] {
  const hoisted = (detail.roles ?? []).filter((r) => r.hoist && !r.is_default).sort((a, b) => b.position - a.position)
  const groups: MemberGroup[] = hoisted.map((role) => ({ role, members: [] }))
  const rest: MemberGroup = { role: null, members: [] }
  for (const m of members) {
    const own = new Set(m.roles ?? [])
    const g = groups.find((g) => own.has(g.role!.id))
    ;(g ?? rest).members.push(m)
  }
  return [...groups.filter((g) => g.members.length), ...(rest.members.length ? [rest] : [])]
}

/** Un membre ne gère que les membres situés sous son rôle le plus élevé ; le créateur gère tout le monde sauf lui-même. */
export function outranks(detail: ServerDetail | undefined, actorId: string, targetId: string): boolean {
  if (!detail || actorId === targetId || targetId === detail.server.owner_id) return false
  if (actorId === detail.server.owner_id) return true
  const find = (id: string) => detail.members.find((m) => m.id === id)
  return topPosition(detail, find(actorId)) > topPosition(detail, find(targetId))
}

/** Rang d'un membre : infini pour le créateur, sinon la position de son rôle le plus élevé. */
export function rankOf(detail: ServerDetail | undefined, userId: string): number {
  if (!detail) return 0
  return userId === detail.server.owner_id ? Infinity : topPosition(detail, detail.members.find((m) => m.id === userId))
}
