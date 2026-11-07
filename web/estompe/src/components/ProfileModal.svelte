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
    <div class="pc-banner" style:--banniere={banner || undefined}></div>
    {#if menu.length}<button class="v-ib pc-menu" bind:this={menuBtn} onclick={() => menuFrom(menuBtn!, menu)} aria-haspopup="menu" aria-label="Plus d'actions" title="Plus d'actions"><Icon name="plus3" size={18} /></button>{/if}
    {#if mine}<input bind:this={colorInput} type="color" hidden value={member?.banner_color || '#808080'} onchange={(e) => void setBanner(e.currentTarget.value)} />{/if}
    <div class="pc-head">
      <Avatar {pseudo} {userId} {serverId} size="lg" status={presence} />
      <div class="pc-name">
        <h2 class="v-titre v-ell" style:color={nameColor(detail, member) || undefined}>{pseudo}</h2>
        {#if isBot(userId)}<RoleBadge kind="bot" />{/if}
        {#if badges.length}<BadgeRow {badges} />{/if}
      </div>
      {#if member?.nickname}<span class="v-mono v-mute">{member.pseudo} · pseudo d'origine</span>{/if}
      <span class="v-note pc-statut"><span class="pc-etat" class:v-surl={presence === 'online'}>{STATUS_LABEL[presence]}</span>{#if activity} · En jeu sur {activity}{/if}</span>
      {#if detail && isTimedOut(member, now)}<span class="v-tag att pc-to"><Icon name="horloge" size={12} />Exclu temporairement jusqu'au {new Date(member!.timeout_until!).toLocaleString('fr-FR')}</span>{/if}
    </div>
    <div class="pc-body">
      {#if detail}
        <div class="pc-sec">
          <span class="v-eyebrow">Rôles</span>
          <div class="pc-chips">
            <span class="v-chip pc-chip">{userId === detail.server.owner_id ? 'Créateur' : 'Membre'}</span>
            {#each memberRoles as r (r.id)}
              <span class="v-chip pc-chip" style:--lisere={r.color || undefined}>{roleLabel(r)}{#if editable && assignable.some((a) => a.id === r.id)}<button class="pc-x" onclick={() => setRole(r.id, false)} aria-label="Retirer le rôle {r.name}" title="Retirer"><Icon name="x" size={12} /></button>{/if}</span>
            {/each}
            {#if editable && addable.length}<button class="v-chip line pc-add" bind:this={addBtn} onclick={pickRole} aria-haspopup="menu" aria-label="Ajouter un rôle" title="Ajouter un rôle"><Icon name="plus" size={12} />Ajouter un rôle</button>{/if}
          </div>
        </div>
        <div class="pc-sec">
          <span class="v-eyebrow">Activité vocale</span>
          <div class="pc-line"><span>Temps passé en vocal</span><b class="v-mono">{times.total < 60 ? 'Jamais venu en vocal' : formatVoiceTotal(times.total)}</b></div>
          {#if times.current !== null}<div class="pc-line"><span>En vocal depuis</span><b class="v-mono">{formatDuration(times.current)}</b></div>{/if}
        </div>
        {#if member?.joined_at}
          <div class="pc-sec">
            <span class="v-eyebrow">Membre depuis</span>
            <div class="pc-line"><span class="v-ell">{detail.server.name}</span><b class="v-mono">{date(member.joined_at)}</b></div>
            {#if member.created_at}<div class="pc-line"><span>Estompe</span><b class="v-mono">{date(member.created_at)}</b></div>{/if}
          </div>
        {/if}
      {/if}
      {#if history.length}
        <div class="pc-sec">
          <span class="v-eyebrow">Historique de modération</span>
          <div class="v-well pc-hist">
            {#each history as e}<p class="v-note"><span class="v-mono">{new Date(e.at).toLocaleDateString('fr-FR')}</span> · {HISTORY[e.action] ?? e.action} par {e.actor}{e.reason ? ` : ${e.reason}` : ''}</p>{/each}
          </div>
        </div>
      {/if}
      <div class="pc-actions">
        {#if mine}
          <button class="plbls-button plbls-button--contour" onclick={() => { onclose(); openSettings('account') }}><Icon name="crayon" size={16} />Modifier mon profil</button>
        {:else if !isBot(userId)}
          <button class="plbls-button" onclick={message}><Icon name="bulle" size={16} />Envoyer un message</button>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .pc-back { position: fixed; inset: 0; z-index: calc(var(--z-modal) + 10); }
  .pc-back.modal { display: grid; place-items: center; padding: 8px; background: color-mix(in srgb, var(--papier) 55%, transparent); backdrop-filter: blur(6px); -webkit-backdrop-filter: blur(6px); }
  .pc { position: fixed; z-index: calc(var(--z-modal) + 11); display: flex; flex-direction: column; width: min(380px, calc(100vw - 16px)); max-height: calc(100dvh - 16px); overflow-y: auto; background: var(--surface); border-radius: var(--r-xl); box-shadow: var(--o3); scrollbar-width: thin; outline: none; animation: v-monte var(--d-l) var(--sortie); }
  .pc:focus-visible { box-shadow: var(--o3), var(--anneau); }
  .pc-back.modal .pc { position: static; }
  .pc-banner { flex: none; height: 100px; background: linear-gradient(180deg, var(--banniere, var(--rose)), var(--surface)); }
  .pc-menu { position: absolute; top: 10px; right: 10px; background: var(--surface); color: var(--encre); box-shadow: var(--o1); }
  .pc-menu:hover:not(:disabled) { background: var(--papier-2); }
  .pc-head { display: flex; flex-direction: column; gap: 4px; padding: 0 24px; margin-top: -48px; min-width: 0; }
  .pc-head :global(.av-wrap .av), .pc-head > :global(.av) { width: 88px; height: 88px; margin: 0 0 6px; border-radius: 28px; font-size: 30px; box-shadow: 0 0 0 5px var(--surface); }
  .pc-head :global(.av-dot--lg) { right: 2px; bottom: 8px; }
  .pc-name { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-width: 0; }
  .pc-name h2 { font-size: 28px; min-width: 0; max-width: 100%; }
  .pc-statut { color: var(--gris); }
  .pc-etat { color: var(--encre); }
  .pc-to { align-self: flex-start; height: auto; min-height: 20px; white-space: normal; margin-top: 4px; }
  .pc-body { display: flex; flex-direction: column; gap: 16px; padding: 16px 24px 24px; }
  .pc-sec { display: flex; flex-direction: column; gap: 6px; }
  .pc-line { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; font-size: 13.5px; }
  .pc-line > span { color: var(--gris); min-width: 0; }
  .pc-line > b { font-weight: 600; text-align: right; }
  .pc-chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .pc-chip { box-shadow: inset 3px 0 0 var(--lisere, transparent); max-width: 100%; }
  .pc-x { display: inline-flex; padding: 2px; margin-right: -4px; border: 0; border-radius: var(--r-p); background: none; color: var(--gris); cursor: pointer; }
  .pc-x:hover { color: var(--encre); background: var(--papier-3); }
  .pc-x:focus-visible, .pc-add:focus-visible { outline: none; box-shadow: var(--anneau); }
  .pc-hist { display: flex; flex-direction: column; gap: 4px; padding: 10px 12px; }
  .pc-hist p { margin: 0; font-size: 12.5px; }
  .pc-actions { display: flex; gap: 8px; }
  .pc-actions .plbls-button { flex: 1; }
  @media (pointer: coarse) { .pc-x { padding: 8px; margin: -6px -8px -6px 0; } .pc-chip, .pc-add { min-height: 36px; } }
  @media (max-width: 640px) { .pc-back.modal { place-items: end center; padding: 0; } .pc-back.modal .pc { width: 100%; border-radius: var(--r-xl) var(--r-xl) 0 0; max-height: 92dvh; } }
</style>
