<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from './Icon.svelte'
  import RoleBadge from './RoleBadge.svelte'
  import BadgeInline from '../blocks/badges/BadgeInline.svelte'
  import { formatVoiceTotal, isBot } from '../lib/format'
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
  let filtre = $state('')
  let now = $state(Date.now())
  onMount(() => { const t = setInterval(() => (now = Date.now()), 10_000); return () => clearInterval(t) })

  const sorted = $derived(
    [...detail.members].sort((a, b) => Number(!!app.online[b.id]) - Number(!!app.online[a.id]) || a.pseudo.localeCompare(b.pseudo, 'fr', { sensitivity: 'base' })),
  )
  const cherche = $derived(filtre.trim().toLocaleLowerCase('fr'))
  const visibles = $derived(cherche ? sorted.filter((m) => `${m.pseudo} ${m.nickname ?? ''}`.toLocaleLowerCase('fr').includes(cherche)) : sorted)
  const groups = $derived(groupMembers(detail, visibles))
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
    {#if sorted.length > 8}
      <label class="members-filtre">
        <Icon name="cherche" size={15} />
        <input type="search" bind:value={filtre} placeholder="Filtrer les membres" aria-label="Filtrer les membres" />
      </label>
    {/if}
    {#each groups as g (g.role?.id ?? '')}
      {#if g.role || groups.length > 1}<div class="side-title role-title" style:color={g.role?.color || undefined}>{g.role ? `${g.role.icon} ${g.role.name}`.trim() : 'Membres'} · {g.members.length}</div>{/if}
      {#each g.members as m (m.id)}
      <button class="nav-item member-row" use:ctx={() => menu(m)} onclick={() => openProfile(m.id, detail.server.id)} title="Profil de {m.pseudo}">
        <Avatar pseudo={m.pseudo} userId={m.id} serverId={detail.server.id} size="sm" status={app.presenceOf(m.id)} />
        <span class="member-body">
          <span class="member-name">
            <span class="member-pseudo truncate" class:off={app.presenceOf(m.id) === 'off'} style:color={nameColor(detail, m)} title={m.nickname ? m.pseudo : undefined}>{shownName(m)}</span>
            {#if isTimedOut(m, now)}<span class="member-ico" role="img" aria-label="Exclu temporairement" title="Exclu temporairement jusqu'à {new Date(m.timeout_until!).toLocaleString('fr-FR')}"><Icon name="lock" size={13} /></span>{/if}
            {#if isBot(m.id)}<RoleBadge kind="bot" />{/if}
            {#if m.id === detail.server.owner_id}<RoleBadge kind="owner" />{/if}
            <BadgeInline userId={m.id} />
          </span>
          {#if app.activityOf(m.id) || app.listeningOf(m.id) || app.voiceStats[detail.server.id]?.[m.id] >= 60}
            <span class="member-sub">
              {#if app.activityOf(m.id)}<span class="truncate" title="Joue à {app.activityOf(m.id)}">🎮 {app.activityOf(m.id)}</span>{/if}
              {#if app.listeningOf(m.id)}<span class="truncate" title="Écoute {app.listeningOf(m.id)}">🎵 Écoute {app.listeningOf(m.id)}</span>{/if}
              {#if app.voiceStats[detail.server.id]?.[m.id] >= 60}<span class="member-time v-mono" title="Temps passé en vocal sur ce serveur">{formatVoiceTotal(app.voiceStats[detail.server.id][m.id])}</span>{/if}
            </span>
          {/if}
        </span>
      </button>
      {/each}
    {:else}
      <p class="members-vide">Aucun membre ne correspond.</p>
    {/each}
  {/if}
</section>

<style>
  .members { display: flex; flex-direction: column; gap: 1px; }
  .members-toggle { display: flex; width: 100%; align-items: center; justify-content: space-between; border: 0; background: transparent; cursor: pointer; text-align: left; border-radius: var(--r-s); }
  .members-toggle:hover { color: var(--encre); }
  .members-filtre { display: flex; align-items: center; gap: var(--espace-2); height: 36px; margin: var(--espace-1) var(--espace-1) var(--espace-2); padding: 0 var(--espace-3); border: 1px solid var(--trait-fort); border-radius: var(--r); background: var(--surface); color: var(--gris-2); }
  .members-filtre:focus-within { border-color: var(--encre); box-shadow: var(--halo); }
  .members-filtre input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--encre); font: inherit; font-size: 14px; }
  .member-row { min-height: 44px; height: auto; padding-block: 4px; }
  .member-body { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .member-name, .member-sub { display: flex; align-items: center; gap: var(--espace-2); min-width: 0; }
  .member-pseudo { min-width: 6ch; color: var(--encre); }
  .member-pseudo.off { opacity: 0.62; }
  .member-ico { display: inline-flex; color: var(--attention); }
  .member-sub { font-size: 11.5px; line-height: 1.25; color: var(--gris); }
  .member-time { flex: none; margin-left: auto; white-space: nowrap; font-size: 11px; }
  .role-title { padding-top: var(--espace-3); }
  .members-vide { margin: 0; padding: var(--espace-3); font-size: 13px; color: var(--gris); }
  @media (pointer: coarse) { .members-toggle { min-height: 44px; } }
  :global(:root[data-density='compacte']) .member-row { min-height: 36px; padding-block: 2px; }
</style>
