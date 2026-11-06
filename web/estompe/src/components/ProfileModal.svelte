<script lang="ts">
  import { onMount } from 'svelte'
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { router, hrefs } from '../lib/route.svelte'
  import type { AuditEntry, Badge } from '../lib/types'
  import { moderationActions } from '../lib/moderation.svelte'
  import { openSettings } from '../lib/settings.svelte'
  import { placeCard } from '../lib/profile.svelte'
  import { menuFrom } from '../lib/ctxmenu.svelte'
  import RoleBadge from './RoleBadge.svelte'
  import BadgeRow from '../blocks/badges/BadgeRow.svelte'
  import { notify } from '../lib/ui'
  import { isTimedOut, nameColor, rankOf, rolesOf, shownName } from '../lib/roles'
  import { STATUS_LABEL, isBot, formatDuration, formatVoiceTotal, voiceTimes } from '../lib/format'

  let { userId, serverId, at = null, onclose }: { userId: string; serverId: string; at?: { x: number; y: number } | null; onclose: () => void } = $props()
  let now = $state(Date.now())
  let badges = $state<Badge[]>([])
  let card = $state<HTMLElement>()
  let menuBtn = $state<HTMLElement>()
  let addBtn = $state<HTMLElement>()
  let colorInput = $state<HTMLInputElement>()
  let pos = $state<{ left: number; top: number } | null>(null)
  // Mode fixé à l'ouverture : la carte ne change pas de forme si la fenêtre est redimensionnée.
  // svelte-ignore state_referenced_locally
  const modal = !at || window.innerWidth < 640
  onMount(() => {
    api.userBadges(userId).then((l) => (badges = l)).catch(() => {})
    const t = setInterval(() => (now = Date.now()), 1000)
    card?.focus()
    return () => clearInterval(t)
  })
  $effect(() => {
    if (!modal && card) pos = placeCard(at!, card.offsetWidth, card.offsetHeight, window.innerWidth, window.innerHeight)
  })

  const detail = $derived(serverId ? app.details[serverId] : undefined)
  const member = $derived(detail?.members.find((m) => m.id === userId))
  const pseudo = $derived(member ? shownName(member) : userId === app.meId ? app.pseudo : '?')
  const actions = $derived(moderationActions(detail, userId))
  let history = $state<AuditEntry[]>([])
  $effect(() => {
    if (serverId && !mine && app.can(serverId, 'view_audit_log')) api.memberHistory(serverId, userId).then((l) => (history = l), () => {})
  })
  const HISTORY: Record<string, string> = { 'member.kick': 'Expulsé', 'member.ban': 'Banni', 'member.unban': 'Ban levé', 'member.timeout': 'Exclu temporairement', 'member.untimeout': 'Exclusion levée', 'member.warn': 'Averti', 'member.nickname': 'Pseudo changé' }
  const presence = $derived(app.presenceOf(userId))
  const activity = $derived(app.activityOf(userId))
  const times = $derived(voiceTimes(app.voiceStats[serverId]?.[userId], app.voiceStatsAt[serverId] ?? 0, app.voiceSinceOf(serverId, userId), now))
  const mine = $derived(userId === app.meId)
  const memberRoles = $derived(rolesOf(detail, member))
  const banner = $derived(member?.banner_color || nameColor(detail, member))
  const date = (s?: string) => (s ? new Date(s).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' }) : '')
  // Mêmes règles que le serveur : « Gérer les rôles », et ne gérer que ce qui est sous son rôle le plus élevé.
  const myTop = $derived(rankOf(detail, app.meId))
  const editable = $derived(!!detail && app.can(serverId, 'manage_roles') && (myTop === Infinity || myTop > rankOf(detail, userId)))
  const assignable = $derived((detail?.roles ?? []).filter((r) => !r.is_default && r.position < myTop))
  const addable = $derived(assignable.filter((r) => !member?.roles?.includes(r.id)))

  async function setRole(roleId: string, on: boolean) {
    try {
      await api.setMemberRole(serverId, userId, roleId, on)
    } catch (e) {
      notify.error(e, 'Rôle non modifié')
    }
  }
  const roleLabel = (r: { icon: string; name: string }) => `${r.icon} ${r.name}`.trim()
  const pickRole = () => menuFrom(addBtn!, addable.map((r) => ({ label: roleLabel(r), run: () => void setRole(r.id, true) })))

  const setBanner = (color: string) => api.setBanner(color).catch((e) => notify.error(e, 'Couleur non enregistrée'))
  const menu = $derived([
    ...(mine ? [{ label: 'Choisir la couleur de la bannière', run: () => colorInput?.click() }, { label: 'Couleur du rôle', run: () => void setBanner('') }] : []),
    ...actions.map((a, i) => ({ label: a.label.replace(/…$/, ''), danger: a.danger, sep: a.danger && !actions[i - 1]?.danger, run: () => { onclose(); a.run() } })),
  ])

  async function message() {
    try {
      const dm = await api.openDM(pseudo === '?' ? '' : member?.pseudo ?? pseudo)
      if (dm.online) app.online[dm.other.id] = true
      if (!app.dms.some((d) => d.id === dm.id)) app.dms.unshift(dm)
      onclose()
      router.go(hrefs.dm(dm.id))
    } catch (e) {
      notify.error(e, 'Message impossible')
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') return void onclose()
    if (e.key !== 'Tab' || !card) return
    const f = [...card.querySelectorAll<HTMLElement>('button, input, [tabindex]:not([tabindex="-1"])')].filter((el) => !el.hasAttribute('disabled') && el.offsetParent)
    if (!f.length) return
    const i = f.indexOf(document.activeElement as HTMLElement)
    const next = e.shiftKey ? (i <= 0 ? f.length - 1 : i - 1) : i === f.length - 1 ? 0 : i + 1
    e.preventDefault()
    f[next].focus()
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="pc-back" class:modal onpointerdown={(e) => { if (e.target === e.currentTarget) onclose() }}>
  <div class="pc" bind:this={card} role="dialog" aria-modal="true" aria-label="Profil de {pseudo}" tabindex="-1" onkeydown={key} style={pos ? `left: ${pos.left}px; top: ${pos.top}px` : modal ? '' : 'visibility: hidden'}>
    <div class="pc-banner" style:background={banner}></div>
    {#if menu.length}<button class="ib sm pc-menu" bind:this={menuBtn} onclick={() => menuFrom(menuBtn!, menu)} aria-haspopup="menu" aria-label="Plus d'actions" title="Plus d'actions"><Icon name="more" size={16} /></button>{/if}
    {#if mine}<input bind:this={colorInput} type="color" hidden value={member?.banner_color || '#808080'} onchange={(e) => void setBanner(e.currentTarget.value)} />{/if}
    <div class="pc-head">
      <Avatar {pseudo} {userId} {serverId} size="lg" status={presence} />
      <span class="pc-name"><strong class="truncate" style:color={nameColor(detail, member)}>{pseudo}</strong>{#if isBot(userId)}<RoleBadge kind="bot" />{/if}{#if badges.length}<BadgeRow {badges} />{/if}</span>
      <span class="small muted">{STATUS_LABEL[presence]}{#if activity} · 🎮 En jeu sur {activity}{/if}</span>
    </div>
    <div class="pc-body">
      {#if detail}
        <div class="pc-sec">
          <span class="pc-title">Rôles</span>
          <div class="pc-chips">
            <span class="plbls-badge">{userId === detail.server.owner_id ? 'Créateur' : 'Membre'}</span>
            {#each memberRoles as r (r.id)}
              <span class="plbls-badge pc-chip"><i style:background={r.color || undefined}></i>{roleLabel(r)}{#if editable && assignable.some((a) => a.id === r.id)}<button onclick={() => setRole(r.id, false)} aria-label="Retirer le rôle {r.name}" title="Retirer"><Icon name="x" size={12} /></button>{/if}</span>
            {/each}
            {#if editable && addable.length}<button class="plbls-badge" bind:this={addBtn} onclick={pickRole} aria-haspopup="menu" aria-label="Ajouter un rôle" title="Ajouter un rôle"><Icon name="plus" size={12} /></button>{/if}
          </div>
        </div>
        {#if member?.nickname}<div class="pc-line"><span class="muted">Pseudo d'origine</span><span>{member.pseudo}</span></div>{/if}
        {#if isTimedOut(member, now)}<div><span class="plbls-badge">Exclu temporairement jusqu'au {new Date(member!.timeout_until!).toLocaleString('fr-FR')}</span></div>{/if}
        <div class="pc-sec">
          <span class="pc-title">Activité vocale</span>
          <div class="pc-line"><span class="muted">Temps passé en vocal</span><span>{times.total < 60 ? 'Jamais venu en vocal' : formatVoiceTotal(times.total)}</span></div>
          {#if times.current !== null}<div class="pc-line"><span class="muted">En vocal depuis</span><span>{formatDuration(times.current)}</span></div>{/if}
        </div>
        {#if member?.joined_at}
          <div class="pc-sec">
            <span class="pc-title">Membre depuis</span>
            <div class="pc-line"><span class="muted truncate">{detail.server.name}</span><span>{date(member.joined_at)}</span></div>
            {#if member.created_at}<div class="pc-line"><span class="muted">Estompe</span><span>{date(member.created_at)}</span></div>{/if}
          </div>
        {/if}
      {/if}
      {#if history.length}
        <div class="pc-sec">
          <span class="pc-title">Historique de modération</span>
          {#each history as e}<p class="small"><span class="muted">{new Date(e.at).toLocaleDateString('fr-FR')}</span> · {HISTORY[e.action] ?? e.action} par {e.actor}{e.reason ? ` : ${e.reason}` : ''}</p>{/each}
        </div>
      {/if}
      <div class="pc-actions">
        {#if mine}
          <button class="plbls-button plbls-button--compact" onclick={() => { onclose(); openSettings('account') }}>Modifier mon profil</button>
        {:else if !isBot(userId)}
          <button class="plbls-button plbls-button--compact" onclick={message}>Envoyer un message</button>
        {/if}
      </div>
    </div>
  </div>
</div>
