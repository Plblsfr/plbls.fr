<script module lang="ts">
  import { app } from '../lib/store.svelte'
  import { notify } from '../lib/ui'
  import { openProfile } from '../lib/profile.svelte'
  import { openSettings } from '../lib/settings.svelte'
  import { openReport } from '../blocks/signalements/report.svelte'
  import type { MenuItem } from '../lib/ctxmenu.svelte'
  import type { Status } from '../lib/types'

  const STATUSES: [Status, string][] = [['online', 'En ligne'], ['idle', 'Absent'], ['dnd', 'Ne pas déranger']]
  export const menuMoi = (serverId: string, extra: MenuItem[] = []): MenuItem[] => [
    ...STATUSES.map(([st, label]) => ({ label, checked: (app.statuses[app.meId] ?? 'online') === st, run: () => void app.setStatus(st).catch((err) => notify.error(err, 'Changement de statut impossible')) })),
    { label: 'Voir mon profil', sep: true, run: () => openProfile(app.meId, serverId) },
    { label: 'Modifier mon profil', run: () => openSettings('account') },
    { label: 'Signaler un bug', run: () => void openReport() },
    ...extra,
  ]
</script>

<script lang="ts">
  import Icon from './Icon.svelte'
  import { draftKeys } from '../lib/drafts.svelte'
  import Watchers from '../blocks/regarder-ensemble/Watchers.svelte'
  import MusicMini from '../blocks/musique/MusicMini.svelte'
  import SoloMini from '../blocks/videos/SoloMini.svelte'
  import { solo } from '../blocks/videos/solo.svelte'
  import { marge } from '../lib/marge.svelte'
  import { participantMenu } from '../lib/volume'
  import { prefs } from '../lib/prefs.svelte'
  import VoiceTimer from './VoiceTimer.svelte'
  import RoleBadge from './RoleBadge.svelte'
  import { STATUS_LABEL, badge, isBot, oldestSince } from '../lib/format'
  import { router, hrefs } from '../lib/route.svelte'
  import { channelKey, dmKey, isVoiceKind } from '../lib/types'
  import { voice } from '../blocks/vocal/voice.svelte'
  import { ctx, ctxmenu, menuFrom } from '../lib/ctxmenu.svelte'
  import ServerIcon from './ServerIcon.svelte'
  import UpdateNotice from './UpdateNotice.svelte'
  import Avatar from './Avatar.svelte'
  import VoiceControls from '../blocks/vocal/VoiceControls.svelte'
  import Quality from './Quality.svelte'
  import { adminSection } from '../lib/admin'
  import { platform } from '../lib/platform'
  import { outranks } from '../lib/roles'
  import { api } from '../lib/api'
  import { channelMod } from '../lib/moderation.svelte'
  import type { Channel, ChannelKind } from '../lib/types'
  import { channelCreate, openNewChannel } from '../lib/channel-create.svelte'
  import FilmsNav from '../blocks/films/FilmsNav.svelte'
  import { videos } from '../blocks/videos/videos.svelte'
  import { sessionPhase, when } from '../blocks/videos/videos'

  let { serverId, activeChannel, onnewdm, onsettings, onserver, onmembres }: {
    serverId: string
    activeChannel?: string
    onnewdm: () => void
    onsettings: () => void
    onserver: () => void
    onmembres: () => void
  } = $props()

  const route = $derived(router.current)
  const space = $derived(route.name === 'space' ? platform.spaces.find((s) => s.id === route.space) : undefined)
  const detail = $derived(serverId ? app.details[serverId] : undefined)
  const voiceChannel = $derived(voice.channelId ? app.channel(voice.channelId) : null)
  const text = $derived(detail?.channels.filter((c) => c.kind === 'text') ?? [])
  // Les salons cinéma sont des salons vocaux : ils suivent les salons vocaux dans la même section.
  const vocal = $derived(detail?.channels.filter((c) => isVoiceKind(c.kind)) ?? [])
  // « Esquisses » : les salons propres à Estompe (Vidéos, Musique…).
  const vids = $derived(detail?.channels.filter((c) => c.kind === 'videos' || c.kind === 'music') ?? [])
  // Séances à venir de tous les salons Esquisses du serveur, dans la barre latérale.
  $effect(() => { for (const c of vids) if (!(c.id in videos.boards)) void videos.load(c.id) })
  const upcoming = $derived(vids.flatMap((c) => (videos.boards[c.id]?.sessions ?? []).filter((s) => sessionPhase(s) === 'upcoming' && s.rsvps[app.meId] !== 'no').map((s) => ({ s, c }))).sort((a, b) => Date.parse(a.s.starts_at) - Date.parse(b.s.starts_at)).slice(0, 3))

  const fail = (e: unknown) => notify.error(e, 'Action impossible')
  /** Déplacer ou déconnecter un participant : permissions « Déplacer / Déconnecter des membres » et rang supérieur au sien. */
  const voiceModItems = (c: Channel, userId: string): MenuItem[] => {
    if (!detail || !outranks(detail, app.meId, userId)) return []
    const sid = detail.server.id
    return [
      ...(app.can(sid, 'move_members') ? vocal.filter((o) => o.id !== c.id).map((o) => ({ label: `Déplacer vers « ${o.name} »`, run: () => void api.voiceMove(c.id, userId, o.id).catch(fail) })) : []),
      ...(app.can(sid, 'disconnect_members') ? [{ label: 'Déconnecter du vocal', danger: true, run: () => void api.voiceDisconnect(c.id, userId).catch(fail) }] : []),
    ]
  }
  // Glisser un participant sur un autre salon vocal le déplace.
  let dragged = $state<{ from: string; user: string } | null>(null)
  const canDrag = (c: Channel, userId: string) => !!detail && app.can(detail.server.id, 'move_members') && outranks(detail, app.meId, userId) && vocal.length > 1
  const canCreate = $derived(!!detail && app.can(detail.server.id, 'manage_channels'))
  const createItem = (kind: ChannelKind, after?: string): MenuItem[] => (detail && canCreate ? [{ label: 'Créer un salon', run: () => openNewChannel(detail.server.id, kind, after) }] : [])
  const SECTION: Record<ChannelKind, string> = { text: 'texte', voice: 'vocal', videos: 'vidéos ou musique', music: 'musique', cinema: 'cinéma' }
  let srvBtn = $state<HTMLElement>()
  const serverMenu = (): MenuItem[] => [...createItem('text'), { label: 'Paramètres du serveur', run: onserver }]
  const channelMenu = (c: Channel): MenuItem[] => [
    ...copy('Copier le nom du salon', c.name),
    ...createItem(c.kind, c.id),
    ...(detail && !isVoiceKind(c.kind) && (app.can(detail.server.id, 'manage_channels') || app.can(detail.server.id, 'manage_messages', c.id)) ? [{ label: 'Modération du salon…', run: () => (channelMod.at = { serverId: detail.server.id, channelId: c.id }) }] : []),
  ]

  let meBtn = $state<HTMLElement>()
  const mic = $derived(voice.muted ? 'Réactiver le micro' : 'Couper le micro')
  const ear = $derived(voice.deafened ? 'Réactiver le son' : 'Sourdine casque')
  const openMeMenu = () => menuFrom(meBtn!, menuMoi(serverId), true)
  const enVocal = $derived(voice.state !== 'idle' && !!voiceChannel)
  const musiqueIci = $derived(enVocal && app.on('musique') && !!voice.channelId && !!app.music[voice.channelId]?.state?.meta)
  const joue = $derived(enVocal || musiqueIci || !!solo.current)
  const sousTitre = $derived(app.activityOf(app.meId) ? `🎮 ${app.activityOf(app.meId)}` : app.listeningOf(app.meId) ? `🎵 Écoute ${app.listeningOf(app.meId)}` : STATUS_LABEL[app.presenceOf(app.meId)])
  const modere = (p: { mic_off?: boolean; sound_off?: boolean }) => !!(p.mic_off || p.sound_off)

  const copy = (label: string, text: string) => [{ label, run: () => void navigator.clipboard.writeText(text).catch((e) => notify.error(e, 'Copie impossible')) }]
</script>

{#snippet title(label: string, kind: ChannelKind, icon?: 'edit')}
  <div class="side-title side-title--add">
    <span class="row">{#if icon}<Icon name={icon} size={12} />{/if}{label}</span>
    {#if canCreate}<button class="ib sm side-add" onclick={() => openNewChannel(detail!.server.id, kind)} aria-label="Créer un salon {SECTION[kind]}" title="Créer un salon {SECTION[kind]}"><Icon name="plus" size={14} /></button>{/if}
  </div>
{/snippet}

{#snippet indic(k: string)}
  {@const n = app.mentionCount(k)}
  {#if draftKeys.has(k)}<span class="draft" title="Brouillon" aria-label="Brouillon"><Icon name="edit" size={13} /></span>{/if}
  {#if n}<span class="pip" aria-label="{n} mentions">{n}</span>{:else if app.unread[k]}<span class="pip pip-nb" aria-label="{app.unread[k]} nouveaux messages">{badge(app.unread[k])}</span>{/if}
{/snippet}

<aside class="side">
  {#if route.name === 'admin' && app.adminSections.length}
    <div class="side-head"><h2>Estompe</h2></div>
    <div class="side-list">
      <div class="side-title">Administration</div>
      {#each app.adminSections as s (s.id)}
        {@const actif = adminSection(route.section, app.adminSections) === s.id}
        <a class="nav-item" class:active={actif} href={hrefs.admin(s.id)} aria-current={actif ? 'page' : undefined}>
          <Icon name={s.icon} size={16} /><span class="truncate">{s.label}</span>
        </a>
      {/each}
    </div>
  {:else if route.name === 'films' && app.movies}
    <FilmsNav />
  {:else if space?.visible()}
    <space.nav />
  {:else if detail}
    <div class="side-head">
      <ServerIcon server={detail.server} size="lg" />
      <h2 class="truncate">{#if canCreate}<button class="side-name" bind:this={srvBtn} onclick={() => menuFrom(srvBtn!, serverMenu())} aria-haspopup="menu">{detail.server.name}<Icon name="bas" size={14} /></button>{:else}{detail.server.name}{/if}</h2>
      <button class="ib" onclick={onmembres} aria-pressed={marge.visible && marge.onglet === 'membres'} aria-label="Membres" title="Membres (Ctrl+.)"><Icon name="gens" /></button>
      <button class="ib" onclick={onserver} aria-label="Paramètres du serveur" title="Paramètres du serveur"><Icon name="sliders" /></button>
    </div>
    <div class="side-list" use:ctx={() => createItem('text')}>
      {@render title('Salons texte', 'text')}
      {#each text as c (c.id)}
        {@const k = channelKey(c.id)}
        <a class="nav-item" use:ctx={() => channelMenu(c)} class:fresh={channelCreate.fresh === c.id} class:active={activeChannel === c.id} class:unread={app.unread[k]} href={hrefs.channel(detail.server.id, c.id)} aria-current={activeChannel === c.id ? 'page' : undefined}>
          <Icon name={c.locked ? 'lock' : 'hash'} size={16} /><span class="truncate">{c.name}</span>
          {@render indic(k)}
        </a>
      {/each}
      {#if vids.length}
        {@render title('Esquisses', 'videos', 'edit')}
        {#each vids as c (c.id)}
          {@const k = channelKey(c.id)}
          <a class="nav-item" use:ctx={() => channelMenu(c)} class:fresh={channelCreate.fresh === c.id} class:active={activeChannel === c.id} class:unread={app.unread[k]} href={hrefs.channel(detail.server.id, c.id)} aria-current={activeChannel === c.id ? 'page' : undefined}>
            <Icon name={c.kind === 'music' ? 'headphones' : 'video'} size={16} /><span class="truncate">{c.name}</span>
            {@render indic(k)}
          </a>
        {/each}
        {#each upcoming as { s, c } (s.id)}
          <a class="nav-item session-item" href={hrefs.channel(detail.server.id, c.id)} title="Séance « {s.title} »">
            <Icon name="horloge" size={14} /><span class="truncate">{s.title}</span><span class="session-when">{when(s.starts_at)}</span>
          </a>
        {/each}
      {/if}
      {@render title('Salons vocaux', 'voice')}
      {#each vocal as c (c.id)}
        <a class="nav-item" use:ctx={() => channelMenu(c)} class:fresh={channelCreate.fresh === c.id} class:active={activeChannel === c.id} class:occupe={app.voice[c.id]?.length} class:drop={!!dragged && dragged.from !== c.id} href={hrefs.channel(detail.server.id, c.id)} aria-current={activeChannel === c.id ? 'page' : undefined} onclick={() => { if (app.can(detail.server.id, 'connect', c.id)) void voice.join(c.id) }}
          ondragover={(e) => { if (dragged && dragged.from !== c.id) e.preventDefault() }}
          ondrop={(e) => { e.preventDefault(); if (dragged && dragged.from !== c.id) void api.voiceMove(dragged.from, dragged.user, c.id).catch(fail); dragged = null }}>
          <Icon name={c.kind === 'cinema' ? 'film' : 'volume'} size={16} /><span class="truncate">{c.name}</span>{#if app.cinema[c.id]}<span class="side-live" title="Film en cours : {app.cinema[c.id].state.meta?.title ?? ''}" role="img" aria-label="Film en cours"><Icon name="film" size={13} /></span>{/if}{#if app.watch[c.id]}<span class="side-live" title="Regarde une vidéo ensemble" role="img" aria-label="Regarde une vidéo ensemble"><Icon name="play" size={12} /></span>{/if}{#if app.music[c.id]}<span class="side-live" title="Écoute de la musique ensemble" role="img" aria-label="Écoute de la musique ensemble"><Icon name="note" size={13} /></span>{/if}
          {#if oldestSince(app.voice[c.id]) !== null}<VoiceTimer since={oldestSince(app.voice[c.id])!} />{/if}
        </a>
        {#if app.on('musique') && app.music[c.id] && !(musiqueIci && voice.channelId === c.id)}<div class="side-musique"><MusicMini channelId={c.id} /></div>{/if}
        {#if app.voice[c.id]?.length}
          <ul class="sub-list">
            {#each app.voice[c.id] as p (p.user_id)}
              <li class="vp" class:parle={!!voice.speaking[p.user_id]} draggable={canDrag(c, p.user_id)} ondragstart={(e) => { if (!canDrag(c, p.user_id)) return e.preventDefault(); dragged = { from: c.id, user: p.user_id }; e.dataTransfer?.setData('text/plain', p.pseudo) }} ondragend={() => (dragged = null)} use:ctx={() => [...participantMenu({
                self: p.user_id === app.meId, muted: !!prefs.mutedVoice[p.user_id], canModerate: outranks(detail, app.meId, p.user_id), canMute: app.can(detail.server.id, 'mute_members', c.id), canDeafen: app.can(detail.server.id, 'deafen_members', c.id), mod: p,
                setMuted: (m) => voice.setUserMuted(p.user_id, m), moderate: (m) => void voice.moderate(c.id, p.user_id, m),
              }), ...voiceModItems(c, p.user_id)]}><Avatar pseudo={p.pseudo} userId={p.user_id} serverId={detail.server.id} size="sm" speaking={!!voice.speaking[p.user_id]} /><span class="truncate grow">{p.pseudo}</span>{#if voice.channelId === c.id}<Watchers list={voice.watchersOf(p.user_id)} />{/if}{#if p.screen}<span class="vp-ico vp-rose" role="img" aria-label="Partage son écran" title="Partage son écran"><Icon name="monitor" size={14} /></span>{:else if p.camera}<span class="vp-ico vp-rose" role="img" aria-label="Caméra allumée" title="Caméra allumée"><Icon name="camera" size={14} /></span>{/if}{#if voice.channelId === c.id}{#if voice.states[p.user_id]?.deaf}<span class="vp-ico" role="img" aria-label="Sourdine casque" title="Sourdine casque"><Icon name="headphones-off" size={14} /></span>{:else if voice.states[p.user_id]?.muted}<span class="vp-ico" role="img" aria-label="Micro coupé" title="Micro coupé"><Icon name="mic-off" size={14} /></span>{/if}{/if}{#if modere(p)}<span class="vp-ico vp-mod" aria-hidden="true"><Icon name="bouclier" size={13} /></span>{/if}{#if p.sound_off}<span class="vp-ico vp-mod" role="img" aria-label="Son coupé par un modérateur" title="Son coupé par un modérateur"><Icon name="headphones-off" size={14} /></span>{/if}{#if p.mic_off}<span class="vp-ico vp-mod" role="img" aria-label="Micro coupé par un modérateur" title="Micro coupé par un modérateur"><Icon name="mic-off" size={14} /></span>{/if}</li>
            {/each}
          </ul>
        {/if}
      {/each}
    </div>
  {:else}
    <div class="side-head">
      <h2>Messages privés</h2>
      <button class="ib" onclick={onnewdm} aria-label="Nouveau message privé" title="Nouveau message privé"><Icon name="plus" /></button>
    </div>
    <div class="side-list">
      <a class="nav-item" class:active={route.name === 'home'} href={hrefs.home()} aria-current={route.name === 'home' ? 'page' : undefined}><Icon name="maison" size={16} /><span class="truncate">Accueil</span></a>
      <div class="side-title">Conversations</div>
      {#each app.dms as d (d.id)}
        {@const k = dmKey(d.id)}
        {@const pr = app.presenceOf(d.other.id)}
        <a class="nav-item dm-item" use:ctx={() => copy('Copier le pseudo', d.other.pseudo)} class:active={route.name === 'dm' && route.id === d.id} class:unread={app.unread[k]} href={hrefs.dm(d.id)} aria-current={route.name === 'dm' && route.id === d.id ? 'page' : undefined}>
          <Avatar pseudo={d.other.pseudo} userId={d.other.id} size="sm" status={pr} />
          <span class="truncate">{d.other.pseudo}</span>{#if isBot(d.other.id)}<RoleBadge kind="bot" />{/if}
          {@render indic(k)}
        </a>
      {:else}
        <p class="side-vide">Aucune conversation. Utilise le bouton « + » pour écrire à quelqu'un.</p>
      {/each}
    </div>
  {/if}

  <div class="dock-w">
    {#if joue}
      <div class="dock">
        {#if musiqueIci && voice.channelId}<MusicMini channelId={voice.channelId} salon />{/if}
        <SoloMini />
        {#if enVocal && voiceChannel}
          <div class="voice-bar" role="region" aria-label="Vocal">
            <div class="voice-head">
              <span class="live-dot" class:wait={voice.state === 'connecting'} aria-hidden="true"></span>
              <a class="voice-where" href={hrefs.channel(voiceChannel.server.id, voiceChannel.channel.id)}>
                <strong>{voice.state === 'connecting' ? 'Connexion…' : 'Vocal connecté'}</strong>
                <span class="truncate">{voiceChannel.channel.name} · {voiceChannel.server.name}</span>
              </a>
              <Quality q={app.meId ? voice.quality[app.meId] : undefined} />
            </div>
            <VoiceControls panel />
          </div>
        {/if}
      </div>
    {/if}

    <UpdateNotice />

    <div class="me-bar">
      <button class="me-id" bind:this={meBtn} onclick={openMeMenu} aria-haspopup="menu" aria-expanded={ctxmenu.at?.up === true} title={app.pseudo}>
        <Avatar pseudo={app.pseudo} userId={app.meId} size="sm" status={app.presenceOf(app.meId)} speaking={!!voice.speaking[app.meId] && !voice.muted} />
        <span class="me-text">
          <strong>{app.pseudo}</strong>
          <span>{sousTitre}</span>
        </span>
      </button>
      {#if voice.state === 'idle'}
      <button class="ib sm" class:off={voice.muted} disabled={voice.state === 'idle'} onclick={() => voice.toggleMute()} aria-label={mic} title={voice.state === 'idle' ? 'Micro (hors vocal)' : mic}><Icon name={voice.muted ? 'mic-off' : 'mic'} size={16} /></button>
      <button class="ib sm" class:off={voice.deafened} disabled={voice.state === 'idle'} onclick={() => voice.toggleDeafen()} aria-label={ear} title={voice.state === 'idle' ? 'Casque (hors vocal)' : ear}><Icon name={voice.deafened ? 'headphones-off' : 'headphones'} size={16} /></button>
      {/if}
      <button class="ib sm" onclick={onsettings} aria-label="Paramètres" title="Paramètres"><Icon name="reglages" size={18} /></button>
    </div>
  </div>
</aside>
