<script lang="ts">
  import { confirmer } from '../lib/confirm.svelte'
  import Icon from './Icon.svelte'
  import { onMount } from 'svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { router, hrefs } from '../lib/route.svelte'
  import { notify } from '../lib/ui'
  import type { AuditEntry, Ban, ChannelKind, ServerDetail, ServerInvite } from '../lib/types'
  import { emojis } from '../blocks/emojis/emojis.svelte'
  import { ctxmenu, menuFrom, type MenuItem } from '../lib/ctxmenu.svelte'
  import IconCrop from './IconCrop.svelte'
  import ServerIcon from './ServerIcon.svelte'
  import Modal from './Modal.svelte'
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
  let tab = $state<'overview' | 'emojis' | 'channels' | 'roles' | 'audit' | 'bans'>('overview')
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

<Modal title={manager ? 'Paramètres du serveur' : detail.server.name} wide {onclose}>
  <div class="stack fill">
    <div class="ss-tabs" role="tablist" aria-label="Sections">
      {#each [['overview', "Vue d'ensemble", true], ['emojis', 'Emojis', true], ['channels', 'Salons', canChannels], ['roles', 'Rôles', canRoles], ['bans', 'Bannis', canBan], ['audit', 'Journal', canAudit]] as [id, label, show]}
        {#if show}<button role="tab" aria-selected={tab === id} class="plbls-button plbls-button--compact" class:plbls-button--contour={tab !== id} onclick={() => (tab = id as typeof tab)}>{label}</button>{/if}
      {/each}
    </div>
    {#if tab === 'roles' && canRoles}
      <RolesSettings {detail} />
    {:else}
    <div class="ss-body">
    {#if tab === 'audit' && canAudit}
      <section class="stack">
        <strong>Journal d'administration</strong>
        <select class="plbls-select" bind:value={auditFilter} aria-label="Filtrer par type d'action">{#each AUDIT_FILTERS as [v, label]}<option value={v}>{label}</option>{/each}</select>
        {#each audit as e}
          <p class="small"><span class="muted">{new Date(e.at).toLocaleString('fr-FR')}</span> · <strong>{e.actor}</strong> {AUDIT_LABEL[e.action] ?? e.action}{e.target ? ` « ${e.target} »` : ''}{e.reason ? ` — ${e.reason}` : ''}</p>
        {:else}
          <p class="muted small">Aucune action enregistrée.</p>
        {/each}
      </section>
    {:else if tab === 'bans' && canBan}
      <section class="stack">
        <strong>Membres bannis</strong>
        {#each bans as b (b.user_id)}
          <div class="row">
            <div class="grow stack">
              <span><strong>{b.pseudo}</strong> <span class="muted small">par {b.by} · {new Date(b.at).toLocaleDateString('fr-FR')} · {b.expires_at ? `jusqu'au ${new Date(b.expires_at).toLocaleString('fr-FR')}` : 'définitif'}</span></span>
              {#if b.reason}<span class="small muted">{b.reason}</span>{/if}
            </div>
            <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => unban(b.user_id)}>Lever le ban</button>
          </div>
        {:else}
          <p class="muted small">Aucun membre banni.</p>
        {/each}
      </section>
    {:else if tab === 'emojis'}
      <section class="stack">
        <div class="row"><strong class="grow">Emojis du serveur</strong><span class="muted small">{mine.length} / {MAX_EMOJIS} emojis</span></div>
        <p class="muted small">Utilisables avec <code>:nom:</code> dans les messages. Image PNG, JPEG ou GIF de 256 px et 128 Ko au plus.</p>
        {#if pending}
          <div class="ss-drop">
            <img src={pending.url} alt="Aperçu de l'emoji" style="width: 64px; height: 64px; object-fit: contain" />
            <div class="plbls-field">
              <label class="plbls-field__label" for="e-name">Nom de l'emoji</label>
              <input id="e-name" class="plbls-input" bind:value={pending.name} maxlength="32" placeholder="mon_emoji" autocomplete="off" />
              <span class="muted small">2 à 32 caractères : a-z, 0-9 et _</span>
            </div>
            <div class="row">
              <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (pending = null)}>Annuler</button>
              <button class="plbls-button plbls-button--compact" disabled={!/^[a-z0-9_]{2,32}$/.test(pending.name)} onclick={addEmoji}>Ajouter</button>
            </div>
          </div>
        {:else if mine.length < MAX_EMOJIS}
          <div class="ss-drop" class:over role="group" aria-label="Ajouter un emoji" ondragover={(e) => { e.preventDefault(); over = true }} ondragleave={() => (over = false)} ondrop={(e) => { e.preventDefault(); over = false; void stageEmoji(e.dataTransfer?.files[0]) }}>
            <span class="small muted">Glisse une image ici</span>
            <label class="plbls-button plbls-button--compact plbls-button--contour">Ajouter un emoji<input type="file" accept="image/png,image/jpeg,image/gif" hidden onchange={pickEmoji} /></label>
          </div>
        {/if}
        <div class="ss-emojis">
          {#each mine as e (e.id)}
            {@const _ = emojis.ensure(e.id)}
            <div class="ss-emoji">
              {#if emojis.src(e.id)}<img src={emojis.src(e.id)} alt="" />{/if}
              {#if e.id in emojiRename}
                <input class="plbls-input" bind:value={emojiRename[e.id]} maxlength="32" aria-label="Nouveau nom de :{e.name}:" onkeydown={(ev) => { if (ev.key === 'Enter') void renameEmoji(e.id); if (ev.key === 'Escape') delete emojiRename[e.id] }} />
              {:else}
                <code>:{e.name}:</code>
              {/if}
              <span class="small muted">{author(e.created_by)}</span>
              {#if canEditEmoji(e)}
                <span class="acts">
                  <button class="ib sm" onclick={() => (e.id in emojiRename ? void renameEmoji(e.id) : (emojiRename[e.id] = e.name))} aria-label={e.id in emojiRename ? 'Valider le nom' : `Renommer :${e.name}:`} title={e.id in emojiRename ? 'Valider' : 'Renommer'}><Icon name={e.id in emojiRename ? 'check' : 'edit'} size={14} /></button>
                  <button class="ib sm" onclick={() => removeEmoji(e.id)} aria-label="Supprimer :{e.name}:" title="Supprimer"><Icon name="trash" size={14} /></button>
                </span>
              {/if}
            </div>
          {:else}
            <p class="muted small" style="grid-column: 1 / -1">Aucun emoji pour l'instant : ajoute le premier !</p>
          {/each}
        </div>
      </section>
    {:else if tab === 'channels' && canChannels}
      <section class="stack">
        <div class="row"><strong class="grow">Salons</strong><button class="plbls-button plbls-button--compact" onclick={() => { openNewChannel(detail.server.id); onclose() }}>Créer un salon</button></div>
        {#each [['text', 'Salons texte'], ['voice', 'Salons vocaux'], ['cinema', 'Salons cinéma'], ['videos', 'Esquisses'], ['music', 'Esquisses']] as [kind, title] (kind)}
          {@const list = ofKind(kind as ChannelKind)}
          {#if kind === 'text' || kind === 'voice' ? true : kind === 'cinema' ? list.length : kind === 'videos' ? list.length || ofKind('music').length : false}<span class="pc-title">{title}</span>{/if}
          {#each list as c (c.id)}
            <div class="ss-chan" class:over={overId === c.id && dragId !== c.id} role="listitem" draggable={!(c.id in renaming)} ondragstart={(e) => { dragId = c.id; e.dataTransfer?.setData('text/plain', c.name) }} ondragend={() => { dragId = overId = null }}
              ondragover={(e) => { if (dragId && ofKind(kind as ChannelKind).some((x) => x.id === dragId)) { e.preventDefault(); overId = c.id } }}
              ondrop={(e) => { e.preventDefault(); if (dragId) reorder(kind as ChannelKind, dragId, c.id); dragId = overId = null }}>
              <span class="grip" aria-hidden="true"><Icon name="menu" size={14} /></span>
              <Icon name={kind === 'voice' ? 'volume' : kind === 'cinema' ? 'film' : kind === 'videos' ? 'video' : kind === 'music' ? 'headphones' : 'hash'} size={16} />
              {#if c.id in renaming}
                <input class="plbls-input grow" bind:value={renaming[c.id]} maxlength="30" aria-label="Nouveau nom de {c.name}" onkeydown={(e) => { if (e.key === 'Enter') void renameChannel(c.id); if (e.key === 'Escape') delete renaming[c.id] }} />
                <button class="plbls-button plbls-button--compact" onclick={() => renameChannel(c.id)}>OK</button>
              {:else}
                <span class="grow truncate">{c.name}</span>
                <span class="acts"><button class="ib sm" onclick={(e) => channelMenu(e.currentTarget, c)} aria-haspopup="menu" aria-label="Actions pour {c.name}" title="Actions"><Icon name="more" size={16} /></button></span>
              {/if}
            </div>
            {#if permsFor === c.id}<ChannelPerms {detail} channelId={c.id} />{/if}
          {/each}
        {/each}
      </section>
    {:else}
      {#if canServer || canIcon}
        <section class="stack">
          <div class="ss-identity">
            {#if canIcon}
              <label class="ss-icon-btn" title="Changer l'icône">
                {#if iconPreview}<span class="plbls-avatar sv-icon"><img class="av-img" src={iconPreview} alt="Nouvelle icône" /></span>
                {:else if iconRemoved}<span class="plbls-avatar sv-icon" aria-hidden="true">{detail.server.name.slice(0, 2)}</span>
                {:else}<ServerIcon server={detail.server} size="lg" />{/if}
                <span class="hint">Changer l'icône</span>
                <input type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden onchange={stageIcon} />
              </label>
            {:else}
              <ServerIcon server={detail.server} size="lg" />
            {/if}
            <div class="plbls-field grow">
              <label class="plbls-field__label" for="s-name">Nom du serveur</label>
              <input id="s-name" class="plbls-input" bind:value={name} maxlength="50" required disabled={!canServer} />
            </div>
          </div>
          {#if canIcon && (iconPreview || (detail.server.icon_version && !iconRemoved))}
            <button class="plbls-button plbls-button--compact plbls-button--danger plbls-button--contour" style="width: fit-content" onclick={() => { iconFile = null; iconRemoved = !!detail.server.icon_version }}>Supprimer l'icône</button>
          {/if}
          <p class="muted small">L'image est recadrée au carré. Sans icône, les initiales du serveur s'affichent.</p>
          <span class="pc-title">Aperçu dans la liste des serveurs</span>
          <div class="ss-preview">
            {#if iconPreview}<span class="plbls-avatar sv-icon"><img class="av-img" src={iconPreview} alt="" /></span>{:else if iconRemoved}<span class="plbls-avatar sv-icon" aria-hidden="true">{name.trim().slice(0, 2)}</span>{:else}<ServerIcon server={{ ...detail.server, name }} />{/if}
            <strong class="truncate">{name.trim() || detail.server.name}</strong>
          </div>
        </section>
      {/if}
    {#if canInvites}
      <section class="stack">
        <strong>Invitations</strong>
        <div class="row">
          <select class="plbls-select" bind:value={inviteExpiry} aria-label="Expiration du lien">{#each EXPIRY as [label, v]}<option value={v}>{label}</option>{/each}</select>
          <select class="plbls-select" bind:value={inviteUses} aria-label="Nombre d'utilisations">{#each USES as [label, v]}<option value={v}>{label}</option>{/each}</select>
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={newInvite}>Créer un lien</button>
        </div>
        {#each invites as i (i.code)}
          <div class="stack">
            <span class="small muted">Par {i.created_by} · {i.uses ?? 0}{i.max_uses ? ` / ${i.max_uses}` : ''} utilisation(s) · {i.expires_at ? `expire le ${new Date(i.expires_at).toLocaleString('fr-FR')}` : 'sans expiration'}</span>
          <div class="row">
            <plbls-copy-field class="grow" value={link(i.code)}></plbls-copy-field>
            <button class="plbls-button plbls-button--compact plbls-button--danger plbls-button--contour" onclick={() => revoke(i.code)}>Révoquer</button>
          </div>
          </div>
        {:else}
          <p class="muted small">Aucun lien actif.</p>
        {/each}
      </section>
    {/if}

    {#if owner}
      <section class="stack">
        <strong>Transférer la propriété</strong>
        <div class="row">
          <select class="plbls-select grow" bind:value={transferTo} aria-label="Nouveau propriétaire"><option value="">Choisir un membre…</option>{#each detail.members.filter((m) => m.id !== app.meId) as m (m.id)}<option value={m.id}>{m.pseudo}</option>{/each}</select>
          <button class="plbls-button plbls-button--danger plbls-button--contour" disabled={!transferTo} onclick={() => (confirmTransfer = true)}>Transférer</button>
        </div>
        <p class="muted small">Vous deviendrez un membre ordinaire du serveur.</p>
      </section>
      <section class="stack">
        <strong>Zone dangereuse</strong>
        <div class="plbls-field">
          <label class="plbls-field__label" for="s-del">Saisis « {detail.server.name} » pour confirmer la suppression</label>
          <input id="s-del" class="plbls-input" bind:value={confirmName} autocomplete="off" />
        </div>
        <button class="plbls-button plbls-button--danger plbls-button--contour" disabled={confirmName !== detail.server.name} onclick={destroy}>Supprimer le serveur</button>
      </section>
    {:else}
      {#if !manager}<p class="muted">Créé par {detail.members.find((m) => m.id === detail.server.owner_id)?.pseudo ?? 'un membre'}. Seuls les rôles autorisés peuvent modifier le serveur.</p>{/if}
      <button class="plbls-button plbls-button--danger plbls-button--contour" onclick={leave}>Quitter le serveur</button>
    {/if}
    {/if}
    {#if dirty}
      <div class="ss-save" role="status">
        <span class="small">Modifications non enregistrées</span>
        <span class="row">
          <button class="plbls-button plbls-button--compact plbls-button--contour" style="color: var(--creme)" onclick={cancelChanges}>Annuler</button>
          <button class="plbls-button plbls-button--compact" disabled={!name.trim()} onclick={save}>Enregistrer</button>
        </span>
      </div>
    {/if}
    </div>
    {/if}
  </div>
</Modal>

{#if cropping}<IconCrop file={cropping} onclose={() => (cropping = null)} onconfirm={(img) => { iconFile = img; iconRemoved = false; cropping = null }} />{/if}

{#if confirmTransfer}
  <Modal title="Transférer le serveur ?" onclose={() => (confirmTransfer = false)}>
    <p>« {detail.server.name} » appartiendra à <strong>{detail.members.find((m) => m.id === transferTo)?.pseudo}</strong>. Vous perdrez tous vos droits de créateur et seul le nouveau propriétaire pourra vous les rendre.</p>
    <div style="display: flex; gap: 8px; justify-content: flex-end">
      <button class="plbls-button plbls-button--contour" onclick={() => (confirmTransfer = false)}>Annuler</button>
      <button class="plbls-button plbls-button--danger" onclick={transfer}>Transférer définitivement</button>
    </div>
  </Modal>
{/if}
