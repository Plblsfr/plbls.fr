<script lang="ts">
  import { confirmer } from '../lib/confirm.svelte'
  import Icon from './Icon.svelte'
  import { onMount } from 'svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { router, hrefs } from '../lib/route.svelte'
  import { notify } from '../lib/ui'
  import { isVoiceKind, type AuditEntry, type Ban, type ChannelKind, type ServerDetail, type ServerInvite } from '../lib/types'
  import { emojis } from '../blocks/emojis/emojis.svelte'
  import { ctxmenu, menuFrom, type MenuItem } from '../lib/ctxmenu.svelte'
  import IconCrop from './IconCrop.svelte'
  import ServerIcon from './ServerIcon.svelte'
  import Modal from './Modal.svelte'
  import Avatar from './Avatar.svelte'
  import type { IconName } from './Icon.svelte'
  import RolesSettings from './RolesSettings.svelte'
  import ChannelPerms from './ChannelPerms.svelte'
  import { openNewChannel } from '../lib/channel-create.svelte'

  let { detail, onclose }: { detail: ServerDetail; onclose: () => void } = $props()

  const owner = $derived(detail.server.owner_id === app.meId)
  const sid = $derived(detail.server.id)
  const canServer = $derived(app.can(sid, 'manage_server'))
  const canChannels = $derived(app.can(sid, 'manage_channels'))
  const canInvites = $derived(app.can(sid, 'create_invites'))
  const canRoles = $derived(app.can(sid, 'manage_roles'))
  const canAudit = $derived(app.can(sid, 'view_audit_log'))
  const canIcon = $derived(canServer || app.isAdmin)
  const canBan = $derived(app.can(sid, 'ban_members'))
  const manager = $derived(canServer || canChannels || canInvites || canRoles || canAudit || canBan || owner)
  type Tab = 'overview' | 'emojis' | 'channels' | 'roles' | 'audit' | 'bans'
  let tab = $state<Tab>('overview')
  const sections = $derived(([
    ['overview', "Vue d'ensemble", 'info', true],
    ['emojis', 'Emojis', 'sourire', true],
    ['channels', 'Salons', 'hash', canChannels],
    ['roles', 'Rôles', 'couronne', canRoles],
    ['bans', 'Bannis', 'interdit', canBan],
    ['audit', 'Journal', 'file', canAudit],
  ] as [Tab, string, IconName, boolean][]).filter((x) => x[3]))
  const tabLabel = $derived(sections.find((x) => x[0] === tab)?.[1] ?? '')
  const role = $derived(owner ? 'Tu es propriétaire' : manager ? 'Tu gères ce serveur' : 'Tu es membre')
  const auditIcon = (a: string): IconName => a === 'member.ban' || a === 'member.kick' ? 'interdit' : a.startsWith('member.timeout') || a === 'member.untimeout' ? 'horloge' : a === 'member.warn' ? 'alerte' : a.startsWith('member') ? 'perso' : a.startsWith('role') ? 'couronne' : a.startsWith('channel') ? 'hash' : a.startsWith('voice') ? 'vol' : a.startsWith('message') ? 'corbeille' : 'reglages'
  const auditTone = (a: string) => a === 'member.ban' || a === 'member.kick' || a.startsWith('message') ? 'dgr' : a === 'member.timeout' || a === 'member.warn' ? 'att' : ''
  const short = (d: string) => new Date(d).toLocaleString('fr-FR', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })
  function onkey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || e.defaultPrevented || cropping || confirmTransfer) return
    if (document.querySelector('plbls-modal, [role="menu"], [aria-modal="true"]')) return
    const t = e.target as HTMLElement | null
    if (t?.closest?.('input:not([readonly]), textarea, select')) return
    onclose()
  }
  let bans = $state<Ban[]>([])
  let auditFilter = $state('')
  let inviteUses = $state(0)
  let inviteExpiry = $state(0)
  let transferTo = $state('')
  let confirmTransfer = $state(false)
  let audit = $state<AuditEntry[]>([])
  let permsFor = $state<string | null>(null)
  const AUDIT_LABEL: Record<string, string> = {
    'role.create': 'a créé le rôle', 'role.update': 'a modifié le rôle', 'role.delete': 'a supprimé le rôle', 'role.reorder': 'a réordonné les rôles',
    'role.assign': 'a attribué le rôle', 'role.unassign': 'a retiré le rôle', 'channel.override': 'a modifié les permissions du salon',
    'channel.moderation': 'a réglé le mode lent / verrouillage de', 'member.kick': 'a expulsé', 'member.ban': 'a banni', 'member.unban': 'a levé le ban de',
    'member.timeout': 'a exclu temporairement', 'member.untimeout': 'a levé l\'exclusion de', 'member.warn': 'a averti', 'member.nickname': 'a changé le pseudo de',
    'voice.move': 'a déplacé en vocal', 'voice.disconnect': 'a déconnecté du vocal', 'message.delete': 'a supprimé un message de', 'message.bulk_delete': 'a supprimé en masse',
    'server.transfer': 'a transféré le serveur à',
  }
  const AUDIT_FILTERS: [string, string][] = [['', 'Toutes les actions'], ['member', 'Modération des membres'], ['role', 'Rôles'], ['channel', 'Salons'], ['voice', 'Vocal'], ['message', 'Messages'], ['server', 'Serveur']]
  $effect(() => { if (tab === 'audit') api.auditLog(sid, { action: auditFilter }).then((l) => (audit = l), (e) => notify.error(e)) })
  $effect(() => { if (tab === 'bans') void refreshBans() })
  const refreshBans = () => api.bans(sid).then((l) => (bans = l), (e) => notify.error(e))
  const unban = (uid: string) => run(async () => { await api.unban(sid, uid); await refreshBans() }, 'Bannissement levé')
  const EXPIRY: [string, number][] = [['Sans expiration', 0], ['30 minutes', 1800], ['1 heure', 3600], ['24 heures', 86400], ['7 jours', 604800]]
  const USES: [string, number][] = [['Illimitées', 0], ['1 utilisation', 1], ['5 utilisations', 5], ['10 utilisations', 10], ['100 utilisations', 100]]
  const transfer = () => run(async () => { await api.transferServer(sid, transferTo); confirmTransfer = false; transferTo = ''; await app.reloadServer(sid) }, 'Serveur transféré')
  // Valeur initiale volontaire : le champ est ensuite libre jusqu'à l'enregistrement.
  // svelte-ignore state_referenced_locally
  let name = $state(detail.server.name)
  let iconFile = $state<Blob | null>(null)
  let iconPreview = $state('')
  let iconRemoved = $state(false)
  let cropping = $state<File | null>(null)
  const dirty = $derived((canServer && name.trim() !== detail.server.name) || !!iconFile || iconRemoved)
  $effect(() => {
    iconPreview = iconFile ? URL.createObjectURL(iconFile) : ''
    const u = iconPreview
    return () => { if (u) URL.revokeObjectURL(u) }
  })
  let invites = $state<ServerInvite[]>([])
  let confirmName = $state('')
  let renaming = $state<Record<string, string>>({})

  const link = (code: string) => `${location.origin}${location.pathname}${hrefs.join(code)}`

  onMount(() => { if (canInvites) void refresh() })

  async function refresh() {
    invites = await api.serverInvites(detail.server.id).catch(() => [])
  }

  async function run(fn: () => Promise<unknown>, ok?: string) {
    try {
      await fn()
      if (ok) notify.ok(ok)
    } catch (e) {
      notify.error(e)
    }
  }

  const mine = $derived(emojis.list.filter((e) => e.server_id === detail.server.id))
  const MAX_EMOJIS = 50
  const author = (id: string) => detail.members.find((m) => m.id === id)?.pseudo ?? 'ancien membre'
  const canEditEmoji = (e: { created_by: string }) => owner || e.created_by === app.meId
  let pending = $state<{ file: File; url: string; name: string } | null>(null)
  let over = $state(false)
  let emojiRename = $state<Record<string, string>>({})
  $effect(() => { const u = pending?.url; return () => { if (u) URL.revokeObjectURL(u) } })

  async function stageEmoji(file?: File) {
    if (!file) return
    if (!['image/png', 'image/jpeg', 'image/gif'].includes(file.type)) return notify.fail('Image PNG, JPEG ou GIF attendue')
    if (file.size > 128 << 10) return notify.fail('Image trop lourde : 128 Ko au plus')
    const bmp = await createImageBitmap(file).catch(() => null)
    if (!bmp) return notify.fail('Image illisible')
    const big = bmp.width > 256 || bmp.height > 256
    bmp.close()
    if (big) return notify.fail('Image trop grande : 256 px au plus')
    const proposed = file.name.replace(/\.[^.]*$/, '').toLowerCase().replace(/[^a-z0-9_]/g, '_').slice(0, 32)
    pending = { file, url: URL.createObjectURL(file), name: proposed.length >= 2 ? proposed : '' }
  }
  const pickEmoji = (e: Event) => {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    void stageEmoji(file)
  }
  const addEmoji = () => run(async () => {
    await api.createEmoji(detail.server.id, pending!.name, pending!.file)
    pending = null
    await emojis.load()
  }, 'Emoji ajouté')
  const renameEmoji = (id: string) => run(async () => { await api.renameEmoji(detail.server.id, id, emojiRename[id].trim().toLowerCase()); delete emojiRename[id]; await emojis.load() })
  const removeEmoji = (id: string) => run(async () => { await api.deleteEmoji(detail.server.id, id); await emojis.load() })

  const stageIcon = (e: Event) => {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (file) cropping = file
  }
  const cancelChanges = () => { name = detail.server.name; iconFile = null; iconRemoved = false }
  const save = () => run(async () => {
    if (canServer && name.trim() !== detail.server.name) await api.renameServer(detail.server.id, name.trim())
    if (iconFile) await api.setServerIcon(detail.server.id, iconFile)
    else if (iconRemoved) await api.deleteServerIcon(detail.server.id)
    iconFile = null
    iconRemoved = false
  }, 'Modifications enregistrées')

  // Salons : ordre par glisser-déposer, ou « Monter » / « Descendre » dans le menu de chaque salon.
  const ofKind = (k: ChannelKind) => detail.channels.filter((c) => c.kind === k)
  let dragId = $state<string | null>(null)
  let overId = $state<string | null>(null)
  const reorder = (kind: ChannelKind, from: string, to: string) => {
    const ids = ofKind(kind).map((c) => c.id)
    const i = ids.indexOf(from)
    if (i < 0 || !ids.includes(to) || from === to) return
    ids.splice(i, 1)
    ids.splice(ids.indexOf(to), 0, from)
    void run(() => api.reorderChannels(detail.server.id, kind, ids))
  }
  const step = (kind: ChannelKind, id: string, d: -1 | 1) => {
    const ids = ofKind(kind).map((c) => c.id)
    const j = ids.indexOf(id) + d
    if (j >= 0 && j < ids.length) reorder(kind, id, ids[j])
  }
  const channelMenu = (btn: Element, c: { id: string; name: string; kind: ChannelKind }) => {
    const ids = ofKind(c.kind).map((x) => x.id)
    const i = ids.indexOf(c.id)
    const items: MenuItem[] = [
      { label: 'Permissions', run: () => (permsFor = permsFor === c.id ? null : c.id) },
      { label: 'Renommer', run: () => (renaming[c.id] = c.name) },
      ...(i > 0 ? [{ label: 'Monter', run: () => step(c.kind, c.id, -1) }] : []),
      ...(i < ids.length - 1 ? [{ label: 'Descendre', run: () => step(c.kind, c.id, 1) }] : []),
      { label: 'Supprimer', danger: true, sep: true, run: () => deleteChannel(c.id, c.name) },
    ]
    menuFrom(btn, items)
  }

  const renameChannel = (id: string) => run(async () => { await api.renameChannel(detail.server.id, id, renaming[id].trim()); delete renaming[id] })
  const deleteChannel = async (id: string, n: string) => { if (await confirmer({ titre: 'Supprimer le salon', message: `Supprimer le salon « ${n} » et tous ses messages ?`, action: 'Supprimer le salon', destructif: true })) void run(() => api.deleteChannel(detail.server.id, id)) }
  const newInvite = () => run(async () => { await api.createServerInvite(detail.server.id, { max_uses: inviteUses || undefined, expires_in: inviteExpiry || undefined }); await refresh() })
  const revoke = (code: string) => run(async () => { await api.revokeServerInvite(detail.server.id, code); await refresh() })

  async function leave() {
    if (!(await confirmer({ titre: 'Quitter le serveur', message: `Quitter « ${detail.server.name} » ?`, action: 'Quitter le serveur', destructif: true }))) return
    await run(async () => {
      await api.leaveServer(detail.server.id)
      app.dropServer(detail.server.id)
      onclose()
      router.go(hrefs.home())
    })
  }

  async function destroy() {
    await run(async () => {
      await api.deleteServer(detail.server.id)
      app.dropServer(detail.server.id)
      onclose()
      router.go(hrefs.home())
    }, 'Serveur supprimé')
  }
</script>

<svelte:window onkeydowncapture={onkey} />

<div class="ssp" role="region" aria-labelledby="ssp-titre">
  <nav class="ssp-nav" aria-label="Sections du serveur">
    <div class="ssp-nav-h">
      <button class="v-ib" onclick={onclose} aria-label="Retour au serveur" title="Retour au serveur"><Icon name="gauche" /></button>
      <b class="grow">{manager ? 'Paramètres du serveur' : 'Serveur'}</b>
    </div>
    <div class="ssp-srv"><ServerIcon server={detail.server} /><b class="v-ell">{detail.server.name}</b></div>
    <div class="ssp-ni-list">
      {#each sections as [id, label, icon] (id)}
        <button class="ssp-ni" aria-current={tab === id ? 'page' : undefined} onclick={() => (tab = id)}>
          <Icon name={icon} size={18} /><span class="grow v-ell">{label}</span>
          {#if id === 'emojis'}<span class="v-mono v-mute">{mine.length}</span>{/if}
        </button>
      {/each}
    </div>
  </nav>

  <div class="ssp-main">
    <header class="ssp-top">
      <h1 id="ssp-titre" class="v-titre grow">{tabLabel}</h1>
      <span class="v-note ssp-role">{role}</span>
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={onclose}>Fermer</button>
    </header>

    {#if tab === 'roles' && canRoles}
      <div class="ssp-roles"><RolesSettings {detail} /></div>
    {:else}
    <div class="ssp-body">
    {#if tab === 'audit' && canAudit}
      <section class="v-card ssp-card">
        <div class="ssp-sec-h">
          <h2 class="ssp-sec-t grow">Journal d'administration</h2>
          <select class="plbls-select ssp-filter" bind:value={auditFilter} aria-label="Filtrer par type d'action">{#each AUDIT_FILTERS as [v, label]}<option value={v}>{label}</option>{/each}</select>
        </div>
        <ul class="ssp-log">
          {#each audit as e}
            <li>
              <time class="v-mono v-mute" datetime={e.at} title={new Date(e.at).toLocaleString('fr-FR')}>{short(e.at)}</time>
              <span class="ssp-log-i {auditTone(e.action)}"><Icon name={auditIcon(e.action)} size={16} /></span>
              <span><b>{e.actor}</b> {AUDIT_LABEL[e.action] ?? e.action}{e.target ? ` « ${e.target} »` : ''}{e.reason ? ` — ${e.reason}` : ''}</span>
            </li>
          {:else}
            <li class="ssp-vide v-note">Aucune action enregistrée.</li>
          {/each}
        </ul>
      </section>
    {:else if tab === 'bans' && canBan}
      <section class="v-card ssp-card">
        <div class="ssp-sec-h"><h2 class="ssp-sec-t grow">Membres bannis</h2><span class="v-mono v-mute">{bans.length}</span></div>
        {#each bans as b (b.user_id)}
          <div class="ssp-srow">
            <Avatar pseudo={b.pseudo} userId={b.user_id} size="sm" />
            <div class="grow ssp-col">
              <b>{b.pseudo}</b>
              <span class="v-note">par {b.by} · {new Date(b.at).toLocaleDateString('fr-FR')} · {b.expires_at ? `jusqu'au ${new Date(b.expires_at).toLocaleString('fr-FR')}` : 'définitif'}</span>
              {#if b.reason}<span class="v-note">« {b.reason} »</span>{/if}
            </div>
            <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => unban(b.user_id)}>Lever le ban</button>
          </div>
        {:else}
          <p class="v-note">Aucun membre banni.</p>
        {/each}
      </section>
    {:else if tab === 'emojis'}
      <section class="v-card ssp-card">
        <div class="ssp-sec-h"><h2 class="ssp-sec-t grow">Emojis du serveur</h2><span class="v-mono v-mute">{mine.length} / {MAX_EMOJIS} emojis</span></div>
        <p class="v-note">Utilisables avec <code class="v-mono">:nom:</code> dans les messages. Image PNG, JPEG ou GIF de 256 px et 128 Ko au plus.</p>
        {#if pending}
          <div class="v-well ssp-pending">
            <img src={pending.url} alt="Aperçu de l'emoji" />
            <div class="plbls-field grow">
              <label class="plbls-field__label" for="e-name">Nom de l'emoji</label>
              <input id="e-name" class="plbls-input v-mono" bind:value={pending.name} maxlength="32" placeholder="mon_emoji" autocomplete="off" />
              <span class="v-note">2 à 32 caractères : a-z, 0-9 et _</span>
            </div>
            <div class="ssp-acts">
              <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (pending = null)}>Annuler</button>
              <button class="plbls-button plbls-button--compact" disabled={!/^[a-z0-9_]{2,32}$/.test(pending.name)} onclick={addEmoji}>Ajouter</button>
            </div>
          </div>
        {:else if mine.length < MAX_EMOJIS}
          <div class="ssp-drop" class:over role="group" aria-label="Ajouter un emoji" ondragover={(e) => { e.preventDefault(); over = true }} ondragleave={() => (over = false)} ondrop={(e) => { e.preventDefault(); over = false; void stageEmoji(e.dataTransfer?.files[0]) }}>
            <span class="ssp-drop-i"><Icon name="envoie" size={28} /></span>
            <span class="grow ssp-col"><b>Glisse une image ici</b><span class="v-note">PNG, JPEG ou GIF · 256 px et 128 Ko au plus · utilisables avec :nom:</span></span>
            <label class="plbls-button plbls-button--compact plbls-button--contour">Ajouter un emoji<input type="file" accept="image/png,image/jpeg,image/gif" hidden onchange={pickEmoji} /></label>
          </div>
        {/if}
        <div class="ssp-emojis">
          {#each mine as e (e.id)}
            {@const _ = emojis.ensure(e.id)}
            <div class="ssp-emoji">
              {#if emojis.src(e.id)}<img src={emojis.src(e.id)} alt="" />{/if}
              {#if e.id in emojiRename}
                <input class="plbls-input v-mono" bind:value={emojiRename[e.id]} maxlength="32" aria-label="Nouveau nom de :{e.name}:" onkeydown={(ev) => { if (ev.key === 'Enter') void renameEmoji(e.id); if (ev.key === 'Escape') delete emojiRename[e.id] }} />
              {:else}
                <code class="v-mono">:{e.name}:</code>
              {/if}
              <span class="v-note">{author(e.created_by)}</span>
              {#if canEditEmoji(e)}
                <span class="acts">
                  <button class="v-ib sm" onclick={() => (e.id in emojiRename ? void renameEmoji(e.id) : (emojiRename[e.id] = e.name))} aria-label={e.id in emojiRename ? 'Valider le nom' : `Renommer :${e.name}:`} title={e.id in emojiRename ? 'Valider' : 'Renommer'}><Icon name={e.id in emojiRename ? 'ok' : 'crayon'} size={14} /></button>
                  <button class="v-ib sm dgr" onclick={() => removeEmoji(e.id)} aria-label="Supprimer :{e.name}:" title="Supprimer"><Icon name="corbeille" size={14} /></button>
                </span>
              {/if}
            </div>
          {:else}
            <p class="v-note" style="grid-column: 1 / -1">Aucun emoji pour l'instant : ajoute le premier !</p>
          {/each}
        </div>
      </section>
    {:else if tab === 'channels' && canChannels}
      <section class="v-card ssp-card">
        <div class="ssp-sec-h"><h2 class="ssp-sec-t grow">Salons</h2><button class="plbls-button plbls-button--compact" onclick={() => { openNewChannel(detail.server.id); onclose() }}><Icon name="plus" size={16} />Créer un salon</button></div>
        {#each [['text', 'Salons texte'], ['videos', 'Esquisses'], ['music', 'Esquisses'], ['voice', 'Salons vocaux'], ['cinema', 'Salons cinéma']] as [kind, title] (kind)}
          {@const list = ofKind(kind as ChannelKind)}
          {#if kind === 'text' || kind === 'voice' ? true : kind === 'cinema' ? list.length : kind === 'videos' ? list.length || ofKind('music').length : false}<span class="v-eyebrow ssp-grp">{title}</span>{/if}
          {#each list as c (c.id)}
            <div class="ssp-chan" class:over={overId === c.id && dragId !== c.id} class:open={permsFor === c.id} role="listitem" draggable={!(c.id in renaming)} ondragstart={(e) => { dragId = c.id; e.dataTransfer?.setData('text/plain', c.name) }} ondragend={() => { dragId = overId = null }}
              ondragover={(e) => { if (dragId && ofKind(kind as ChannelKind).some((x) => x.id === dragId)) { e.preventDefault(); overId = c.id } }}
              ondrop={(e) => { e.preventDefault(); if (dragId) reorder(kind as ChannelKind, dragId, c.id); dragId = overId = null }}>
              <span class="grip" aria-hidden="true"><Icon name="menu" size={14} /></span>
              <Icon name={kind === 'voice' ? 'vol' : kind === 'cinema' ? 'film' : kind === 'videos' ? 'video' : kind === 'music' ? 'casque' : 'hash'} size={18} />
              {#if c.id in renaming}
                <input class="plbls-input grow" bind:value={renaming[c.id]} maxlength="30" aria-label="Nouveau nom de {c.name}" onkeydown={(e) => { if (e.key === 'Enter') void renameChannel(c.id); if (e.key === 'Escape') delete renaming[c.id] }} />
                <button class="plbls-button plbls-button--compact" onclick={() => renameChannel(c.id)}>OK</button>
              {:else}
                <span class="grow v-ell">{c.name}</span>
                {#if c.locked}<span class="v-tag">verrouillé</span>{/if}
                {#if isVoiceKind(c.kind) && c.user_limit}<span class="v-mono v-mute">{c.user_limit} max</span>{/if}
                <span class="acts"><button class="v-ib" class:on={permsFor === c.id} onclick={(e) => channelMenu(e.currentTarget, c)} aria-haspopup="menu" aria-label="Actions pour {c.name}" title="Actions"><Icon name="plus3" size={18} /></button></span>
              {/if}
            </div>
            {#if permsFor === c.id}<ChannelPerms {detail} channelId={c.id} />{/if}
          {/each}
        {/each}
      </section>
    {:else}
      {#if canServer || canIcon}
        <section class="ssp-sec">
          <h2 class="ssp-sec-t">Identité</h2>
          <div class="ssp-identity">
            {#if canIcon}
              <label class="ssp-icon-btn" title="Changer l'icône">
                <span class="ssp-icon">
                  {#if iconPreview}<span class="plbls-avatar sv-icon"><img class="av-img" src={iconPreview} alt="Nouvelle icône" /></span>
                  {:else if iconRemoved}<span class="plbls-avatar sv-icon" aria-hidden="true">{detail.server.name.slice(0, 2)}</span>
                  {:else}<ServerIcon server={detail.server} size="lg" />{/if}
                  <span class="ssp-icon-edit" aria-hidden="true"><Icon name="crayon" size={16} /></span>
                </span>
                <span class="v-note">Changer l'icône</span>
                <input type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden onchange={stageIcon} />
              </label>
            {:else}
              <span class="ssp-icon"><ServerIcon server={detail.server} size="lg" /></span>
            {/if}
            <div class="grow ssp-col ssp-gap">
              <div class="plbls-field">
                <label class="plbls-field__label" for="s-name">Nom du serveur</label>
                <input id="s-name" class="plbls-input" bind:value={name} maxlength="50" required disabled={!canServer} />
                {#if canServer}<span class="v-mono v-mute ssp-count">{[...name].length} / 50</span>{/if}
              </div>
              <div class="ssp-preview">
                <span class="v-note">Aperçu dans la liste des serveurs</span>
                <span class="ssp-rail">
                  {#if iconPreview}<span class="plbls-avatar sv-icon"><img class="av-img" src={iconPreview} alt="" /></span>{:else if iconRemoved}<span class="plbls-avatar sv-icon" aria-hidden="true">{name.trim().slice(0, 2)}</span>{:else}<ServerIcon server={{ ...detail.server, name }} />{/if}
                </span>
                <b class="v-ell">{name.trim() || detail.server.name}</b>
                <span class="grow"></span>
                {#if canIcon && (iconPreview || (detail.server.icon_version && !iconRemoved))}
                  <button class="plbls-button plbls-button--compact plbls-button--discret ssp-dgr" onclick={() => { iconFile = null; iconRemoved = !!detail.server.icon_version }}>Supprimer l'icône</button>
                {/if}
              </div>
              <p class="v-note">L'image est recadrée au carré. Sans icône, les initiales du serveur s'affichent.</p>
            </div>
          </div>
        </section>
        <hr class="v-hr" />
      {/if}
    {#if canInvites}
      <section class="ssp-sec">
        <h2 class="ssp-sec-t">Invitations</h2>
        <div class="ssp-inv-form">
          <div class="plbls-field"><label class="plbls-field__label" for="s-exp">Expiration du lien</label><select id="s-exp" class="plbls-select" bind:value={inviteExpiry}>{#each EXPIRY as [label, v]}<option value={v}>{label}</option>{/each}</select></div>
          <div class="plbls-field"><label class="plbls-field__label" for="s-uses">Nombre d'utilisations</label><select id="s-uses" class="plbls-select" bind:value={inviteUses}>{#each USES as [label, v]}<option value={v}>{label}</option>{/each}</select></div>
          <button class="plbls-button" onclick={newInvite}>Créer un lien</button>
        </div>
        {#each invites as i (i.code)}
          <div class="v-card ssp-invite">
            <div class="ssp-inv-row">
              <plbls-copy-field class="grow" label="Lien d'invitation" value={link(i.code)}></plbls-copy-field>
              <button class="plbls-button plbls-button--compact plbls-button--discret ssp-dgr" onclick={() => revoke(i.code)}>Révoquer</button>
            </div>
            <span class="v-mono v-mute">Par {i.created_by} · {i.uses ?? 0}{i.max_uses ? ` / ${i.max_uses}` : ''} utilisation(s) · {i.expires_at ? `expire le ${new Date(i.expires_at).toLocaleString('fr-FR')}` : 'sans expiration'}</span>
          </div>
        {:else}
          <p class="v-note">Aucun lien actif.</p>
        {/each}
      </section>
      <hr class="v-hr" />
    {/if}

    {#if owner}
      <section class="ssp-sec">
        <h2 class="ssp-sec-t">Transférer la propriété</h2>
        <div class="ssp-inv-row">
          <select class="plbls-select grow" bind:value={transferTo} aria-label="Nouveau propriétaire"><option value="">Choisir un membre…</option>{#each detail.members.filter((m) => m.id !== app.meId) as m (m.id)}<option value={m.id}>{m.pseudo}</option>{/each}</select>
          <button class="plbls-button plbls-button--contour" disabled={!transferTo} onclick={() => (confirmTransfer = true)}>Transférer</button>
        </div>
        <p class="v-note">Tu deviendras un membre ordinaire du serveur.</p>
      </section>
      <section class="ssp-sec ssp-danger">
        <h2 class="ssp-sec-t">Zone dangereuse</h2>
        <div class="plbls-field">
          <label class="plbls-field__label" for="s-del">Saisis « {detail.server.name} » pour confirmer la suppression</label>
          <div class="ssp-inv-row">
            <input id="s-del" class="plbls-input grow" bind:value={confirmName} autocomplete="off" />
            <button class="plbls-button plbls-button--danger" disabled={confirmName !== detail.server.name} onclick={destroy}>Supprimer le serveur</button>
          </div>
        </div>
      </section>
    {:else}
      <section class="ssp-sec">
        {#if !manager}<p class="v-note">Créé par {detail.members.find((m) => m.id === detail.server.owner_id)?.pseudo ?? 'un membre'}. Seuls les rôles autorisés peuvent modifier le serveur.</p>{/if}
        <button class="plbls-button plbls-button--danger plbls-button--contour ssp-fit" onclick={leave}><Icon name="sortie" size={16} />Quitter le serveur</button>
      </section>
    {/if}
    {/if}
    </div>
    {/if}

    {#if dirty && tab !== 'roles'}
      <div class="v-toast ssp-save" role="status">
        <span class="ssp-save-i"><Icon name="alerte" size={18} /></span>
        <span class="grow">Modifications non enregistrées</span>
        <button class="plbls-button plbls-button--compact plbls-button--discret ssp-save-annuler" onclick={cancelChanges}>Annuler</button>
        <button class="plbls-button plbls-button--compact v-rose" disabled={!name.trim()} onclick={save}>Enregistrer</button>
      </div>
    {/if}
  </div>
</div>

{#if cropping}<IconCrop file={cropping} onclose={() => (cropping = null)} onconfirm={(img) => { iconFile = img; iconRemoved = false; cropping = null }} />{/if}

{#if confirmTransfer}
  <Modal title="Transférer le serveur ?" onclose={() => (confirmTransfer = false)}>
    <p>« {detail.server.name} » appartiendra à <strong>{detail.members.find((m) => m.id === transferTo)?.pseudo}</strong>. Tu perdras tous tes droits de créateur et seul le nouveau propriétaire pourra te les rendre.</p>
    <div class="ssp-modal-f">
      <button class="plbls-button plbls-button--contour" onclick={() => (confirmTransfer = false)}>Annuler</button>
      <button class="plbls-button plbls-button--danger" onclick={transfer}>Transférer définitivement</button>
    </div>
  </Modal>
{/if}

<style>
  .ssp { flex: 1; min-height: 0; display: grid; grid-template-columns: 240px minmax(0, 1fr); background: var(--papier); }
  .ssp-nav { display: flex; flex-direction: column; gap: 2px; min-height: 0; overflow-y: auto; padding: 0 10px 16px; background: var(--papier-2); box-shadow: inset -1px 0 0 var(--trait); scrollbar-width: thin; }
  .ssp-nav-h { display: flex; align-items: center; gap: 8px; min-height: 60px; padding-top: env(safe-area-inset-top); }
  .ssp-nav-h b { min-width: 0; overflow: hidden; font-family: var(--f-titre); font-size: 15px; letter-spacing: -0.02em; white-space: nowrap; text-overflow: ellipsis; }
  .ssp-srv { display: flex; align-items: center; gap: 10px; padding: 6px 10px 14px; min-width: 0; }
  .ssp-srv :global(.sv-icon) { width: 36px; height: 36px; border-radius: 11px; font-size: 13px; }
  .ssp-ni-list { display: flex; flex-direction: column; gap: 2px; }
  .ssp-ni { display: flex; align-items: center; gap: 10px; width: 100%; height: 38px; padding: 0 10px; border: 0; border-radius: var(--r-s); background: transparent; color: var(--encre-2); font: inherit; font-size: 14px; text-align: left; cursor: pointer; transition: background var(--d) var(--sortie); }
  .ssp-ni:hover { background: var(--papier-3); color: var(--encre); }
  .ssp-ni:focus-visible { outline: none; box-shadow: var(--anneau); }
  .ssp-ni[aria-current='page'] { background: var(--rose); color: var(--encre); font-weight: 600; }
  .ssp-main { position: relative; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--surface); }
  .ssp-top { display: flex; align-items: center; gap: 12px; min-height: 60px; padding: env(safe-area-inset-top) 16px 0 32px; box-shadow: 0 1px 0 var(--trait); }
  .ssp-top h1 { font-size: 22px; }
  .ssp-body { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 28px; padding: 28px 40px 110px; max-width: 960px; scrollbar-width: thin; }
  .ssp-roles { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .ssp-sec { display: flex; flex-direction: column; gap: 14px; }
  .ssp-sec-t { margin: 0; font-family: var(--f-titre); font-size: 20px; font-weight: 700; letter-spacing: -0.02em; }
  .ssp-sec-h { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .ssp-card { display: flex; flex-direction: column; gap: 10px; }
  .ssp-col { display: flex; flex-direction: column; min-width: 0; }
  .ssp-gap { gap: 12px; }
  .ssp-acts { display: flex; gap: 8px; flex-wrap: wrap; }
  .ssp-fit { width: fit-content; }
  .ssp-dgr { color: var(--danger); }
  .ssp-filter { width: auto; min-width: 200px; }
  .ssp-identity { display: flex; align-items: flex-start; gap: 24px; flex-wrap: wrap; }
  .ssp-icon-btn { display: flex; flex-direction: column; align-items: center; gap: 8px; cursor: pointer; border-radius: var(--r); }
  .ssp-icon-btn:focus-within { box-shadow: var(--anneau); }
  .ssp-icon { position: relative; display: inline-flex; }
  .ssp-icon :global(.sv-icon) { width: 96px; height: 96px; border-radius: 28px; font-size: 32px; }
  .ssp-icon-edit { position: absolute; right: -6px; bottom: -6px; display: grid; place-items: center; width: 32px; height: 32px; border-radius: 50%; background: var(--encre); color: var(--papier); box-shadow: 0 0 0 3px var(--surface); }
  .ssp-count { align-self: flex-end; }
  .ssp-preview { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; min-width: 0; }
  .ssp-rail { display: inline-flex; }
  .ssp-rail :global(.sv-icon) { width: 40px; height: 40px; border-radius: 12px; font-size: 13px; box-shadow: inset 0 -3px 0 var(--rose-fonce); }
  .ssp-inv-form { display: flex; align-items: flex-end; gap: 10px; flex-wrap: wrap; }
  .ssp-inv-form .plbls-field { flex: 1 1 200px; }
  .ssp-invite { display: flex; flex-direction: column; gap: 10px; padding: 16px; }
  .ssp-inv-row { display: flex; align-items: flex-end; gap: 10px; flex-wrap: wrap; }
  .ssp-inv-row > :global(plbls-copy-field) { min-width: min(100%, 240px); }
  .ssp-danger { padding: 20px; border-radius: var(--r-l); box-shadow: inset 0 0 0 1.5px var(--danger); }
  .ssp-danger .ssp-sec-t { color: var(--danger); }
  .ssp-srow { display: flex; align-items: center; gap: 12px; padding: 10px 0; box-shadow: 0 1px 0 var(--trait); }
  .ssp-srow:last-child { box-shadow: none; }
  .ssp-log { list-style: none; margin: 0; padding: 0; }
  .ssp-log li { display: grid; grid-template-columns: 96px 26px minmax(0, 1fr); gap: 10px; align-items: center; padding: 10px 0; box-shadow: 0 1px 0 var(--trait); font-size: 14px; overflow-wrap: anywhere; }
  .ssp-log li:last-child { box-shadow: none; }
  .ssp-log li.ssp-vide { display: block; }
  .ssp-log-i { display: inline-flex; color: var(--gris); }
  .ssp-log-i.dgr { color: var(--danger); }
  .ssp-log-i.att { color: var(--attention); }
  .ssp-pending { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .ssp-pending img { width: 48px; height: 48px; object-fit: contain; border-radius: 12px; background: var(--surface); }
  .ssp-drop { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; padding: 20px; border: 1.5px dashed var(--trait-fort); border-radius: var(--r-l); }
  .ssp-drop.over { border-color: var(--encre); background: var(--rose-doux); }
  .ssp-drop-i { display: inline-flex; color: var(--rose-texte); }
  .ssp-emojis { display: grid; grid-template-columns: repeat(auto-fill, minmax(112px, 1fr)); gap: 8px; }
  .ssp-emoji { position: relative; display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 0; padding: 10px; border-radius: var(--r); background: var(--papier-2); text-align: center; }
  .ssp-emoji:focus-within { box-shadow: inset 0 0 0 2px var(--encre); }
  .ssp-emoji img { width: 36px; height: 36px; object-fit: contain; }
  .ssp-emoji code, .ssp-emoji .v-note { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; }
  .ssp-emoji .acts { position: absolute; top: 2px; right: 2px; display: none; gap: 2px; padding: 2px; border-radius: var(--r-s); background: var(--surface); box-shadow: var(--o1); }
  .ssp-emoji:hover .acts, .ssp-emoji:focus-within .acts { display: flex; }
  .v-ib.dgr { color: var(--danger); }
  .ssp-grp { padding: 12px 10px 4px; }
  .ssp-chan { display: flex; align-items: center; gap: 10px; min-height: 44px; padding: 0 6px 0 10px; border-radius: var(--r-s); }
  .ssp-chan:hover { background: var(--papier-2); }
  .ssp-chan.open { background: var(--rose-doux); }
  .ssp-chan.over { box-shadow: inset 0 2px 0 var(--encre); }
  .ssp-chan .acts { display: flex; opacity: 0; }
  .ssp-chan:hover .acts, .ssp-chan:focus-within .acts, .ssp-chan.open .acts { opacity: 1; }
  .ssp-chan .grip { display: inline-flex; cursor: grab; color: var(--gris-2); }
  .ssp-save { position: absolute; left: 50%; bottom: 20px; transform: translateX(-50%); width: min(720px, calc(100% - 32px)); flex-wrap: wrap; }
  .ssp-save-i { display: inline-flex; color: var(--rose); }
  .ssp-save-annuler { color: var(--papier); }
  .ssp-save-annuler:hover:not(:disabled) { background: color-mix(in srgb, var(--papier) 14%, transparent); color: var(--papier); }
  .ssp-modal-f { display: flex; gap: 8px; justify-content: flex-end; flex-wrap: wrap; }
  @media (hover: none) { .ssp-chan .acts, .ssp-emoji .acts { opacity: 1; display: flex; } }
  @media (max-width: 760px) {
    .ssp { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); }
    .ssp-nav { overflow: visible; padding: 0 12px 8px; box-shadow: inset 0 -1px 0 var(--trait); }
    .ssp-srv { padding: 0 4px 8px; }
    .ssp-srv :global(.sv-icon) { width: 28px; height: 28px; border-radius: 9px; font-size: 11px; }
    .ssp-ni-list { flex-direction: row; gap: 4px; overflow-x: auto; scrollbar-width: none; }
    .ssp-ni { width: auto; flex: none; }
    .ssp-top { padding: 0 12px 0 16px; min-height: 52px; }
    .ssp-role { display: none; }
    .ssp-body { padding: 20px 16px 110px; }
    .ssp-log li { grid-template-columns: 80px 22px minmax(0, 1fr); }
  }
  @media (pointer: coarse) {
    .ssp-ni { height: 44px; }
    .ssp-chan .v-ib, .ssp-emoji .v-ib { width: 44px; height: 44px; }
  }
</style>
