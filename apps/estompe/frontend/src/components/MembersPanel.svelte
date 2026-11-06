<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from './Icon.svelte'
  import RoleBadge from './RoleBadge.svelte'
  import BadgeInline from '../blocks/badges/BadgeInline.svelte'
  import { STATUS_LABEL, formatVoiceTotal, isBot } from '../lib/format'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { router, hrefs } from '../lib/route.svelte'
  import { notify } from '../lib/ui'
  import type { ServerDetail } from '../lib/types'
  import Avatar from './Avatar.svelte'
  import { openProfile } from '../lib/profile.svelte'
  import { ctx, type MenuItem } from '../lib/ctxmenu.svelte'
  import { groupMembers, isTimedOut, nameColor, shownName } from '../lib/roles'
  import { moderationActions } from '../lib/moderation.svelte'
  
  let { detail }: { detail: ServerDetail } = $props()
  let open = $state(true)
  let now = $state(Date.now())
  onMount(() => { const t = setInterval(() => (now = Date.now()), 10_000); return () => clearInterval(t) })

  const sorted = $derived(
    [...detail.members].sort((a, b) => Number(!!app.online[b.id]) - Number(!!app.online[a.id]) || a.pseudo.localeCompare(b.pseudo, 'fr', { sensitivity: 'base' })),
  )
  const groups = $derived(groupMembers(detail, sorted))
  const onlineCount = $derived(sorted.filter((m) => app.presenceOf(m.id) !== 'off').length)

  async function message(pseudo: string, id: string) {
    if (id === app.meId) return
    try {
      const dm = await api.openDM(pseudo)
      if (dm.online) app.online[dm.other.id] = true
      if (!app.dms.some((d) => d.id === dm.id)) app.dms.unshift(dm)
      router.go(hrefs.dm(dm.id))
    } catch (e) {
      notify.error(e, 'Impossible d\'ouvrir la conversation')
    }
  }

  const copy = (text: string) => navigator.clipboard.writeText(text).catch((e) => notify.error(e, 'Copie impossible'))
  const menu = (m: { id: string; pseudo: string }): MenuItem[] => [
    { label: 'Voir le profil', run: () => openProfile(m.id, detail.server.id) },
    ...(m.id === app.meId ? [] : [{ label: 'Écrire un message', run: () => void message(m.pseudo, m.id) }]),
    { label: 'Copier le pseudo', run: () => void copy(m.pseudo) },
    ...(app.can(detail.server.id, 'manage_roles') ? [{ label: 'Gérer les rôles', run: () => openProfile(m.id, detail.server.id) }] : []),
    ...moderationActions(detail, m.id),
  ]
</script>

<section class="members" aria-label="Membres">
  <button class="side-title members-toggle" aria-expanded={open} onclick={() => (open = !open)}>
    <span>Membres · {onlineCount}/{sorted.length}</span>
    <Icon name={open ? 'minus' : 'plus'} size={14} />
  </button>
  {#if open}
    {#each groups as g (g.role?.id ?? '')}
      {#if g.role || groups.length > 1}<div class="side-title role-title" style:color={g.role?.color || undefined}>{g.role ? `${g.role.icon} ${g.role.name}`.trim() : 'Membres'} · {g.members.length}</div>{/if}
      {#each g.members as m (m.id)}
      <button class="nav-item member-row" use:ctx={() => menu(m)} onclick={() => openProfile(m.id, detail.server.id)} title="Profil de {m.pseudo}">
        <span class="dot {app.presenceOf(m.id) === 'online' ? 'online' : app.presenceOf(m.id)}" role="img" aria-label={STATUS_LABEL[app.presenceOf(m.id)]} title={STATUS_LABEL[app.presenceOf(m.id)]}></span>
        <Avatar pseudo={m.pseudo} userId={m.id} serverId={detail.server.id} size="sm" />
        <span class="member-body">
          <span class="member-name">
            <span class="member-pseudo truncate" class:muted={app.presenceOf(m.id) === 'off'} style:color={nameColor(detail, m)} title={m.nickname ? m.pseudo : undefined}>{shownName(m)}</span>
            {#if isTimedOut(m, now)}<span role="img" aria-label="Exclu temporairement" title="Exclu temporairement jusqu'à {new Date(m.timeout_until!).toLocaleString('fr-FR')}"><Icon name="lock" size={13} /></span>{/if}
            {#if isBot(m.id)}<RoleBadge kind="bot" />{/if}
            {#if m.id === detail.server.owner_id}<RoleBadge kind="owner" />{/if}
            <BadgeInline userId={m.id} />
          </span>
          {#if app.activityOf(m.id) || app.listeningOf(m.id) || app.voiceStats[detail.server.id]?.[m.id] >= 60}
            <span class="member-sub small muted">
              {#if app.activityOf(m.id)}<span class="truncate" title="Joue à {app.activityOf(m.id)}">🎮 {app.activityOf(m.id)}</span>{/if}
              {#if app.listeningOf(m.id)}<span class="truncate" title="Écoute {app.listeningOf(m.id)}">🎵 Écoute {app.listeningOf(m.id)}</span>{/if}
              {#if app.voiceStats[detail.server.id]?.[m.id] >= 60}<span class="member-time" title="Temps passé en vocal sur ce serveur">{formatVoiceTotal(app.voiceStats[detail.server.id][m.id])}</span>{/if}
            </span>
          {/if}
        </span>
      </button>
      {/each}
    {/each}
  {/if}
</section>
