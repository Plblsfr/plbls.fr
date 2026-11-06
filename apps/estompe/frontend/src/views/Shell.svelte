<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { notify } from '../lib/ui'
  import { router } from '../lib/route.svelte'
  import { channelKey, dmKey, isVoiceKind } from '../lib/types'
  import ServerRail from '../components/ServerRail.svelte'
  import SidePanel from '../components/SidePanel.svelte'
  import MusicDock from '../blocks/musique/MusicDock.svelte'
  import SoloDock from '../blocks/videos/SoloDock.svelte'
  import ChatView from '../components/ChatView.svelte'
  import VoiceChannelView from '../blocks/vocal/VoiceChannelView.svelte'
  import VideosView from '../blocks/videos/VideosView.svelte'
  import HomeView from './HomeView.svelte'
  import JoinView from './JoinView.svelte'
  import AdminSpace from '../components/AdminSpace.svelte'
  import FilmsSpace from '../blocks/films/FilmsSpace.svelte'
  import CreateJoinModal from '../components/CreateJoinModal.svelte'
  import NewDMModal from '../components/NewDMModal.svelte'
  import SettingsModal from '../components/SettingsModal.svelte'
  import { saveLastView } from '../lib/lastview'
  import { settingsNav } from '../lib/settings.svelte'
  import ProfileModal from '../components/ProfileModal.svelte'
  import { profileNav } from '../lib/profile.svelte'
  import { channelMod, moderation } from '../lib/moderation.svelte'
  import ModerationModal from '../components/ModerationModal.svelte'
  import ChannelModModal from '../components/ChannelModModal.svelte'
  import CreateChannelModal from '../components/CreateChannelModal.svelte'
  import { channelCreate } from '../lib/channel-create.svelte'
  import ServerSettingsModal from '../components/ServerSettingsModal.svelte'
  import ReportModal from '../blocks/signalements/ReportModal.svelte'
  import { openReport, reportNav } from '../blocks/signalements/report.svelte'
  import { platform } from '../lib/platform'
  import { searchNav, openSearch } from '../lib/search.svelte'
  import SearchModal from '../components/SearchModal.svelte'

  let nav = $state(false)
  let modal = $state<'' | 'create' | 'dm' | 'settings' | 'server'>('')

  $effect(() => { if (settingsNav.request) modal = 'settings' })
  $effect(() => {
    if (!channelCreate.done) return
    const t = setTimeout(() => (channelCreate.done = null), 8000)
    return () => clearTimeout(t)
  })

  const route = $derived(router.current)
  const serverId = $derived(route.name === 'channel' ? route.serverId : '')
  const detail = $derived(serverId ? app.details[serverId] : undefined)

  // Salon à afficher : celui de l'URL, sinon le premier salon texte du serveur.
  const channel = $derived.by(() => {
    if (route.name !== 'channel' || !detail) return undefined
    const list = detail.channels
    return list.find((c) => c.id === route.channelId) ?? list.find((c) => c.kind === 'text') ?? list[0]
  })

  const convKey = $derived.by(() => {
    if (route.name === 'dm') return dmKey(route.id)
    if (channel?.kind === 'text') return channelKey(channel.id)
    return null
  })

  $effect(() => {
    searchNav.convKey = convKey ?? ''
    searchNav.serverId = serverId
  })

  $effect(() => {
    const key = convKey
    nav = false
    void app.open(key).catch(() => {})
  })

  $effect(() => {
    if (route.name === 'channel' || route.name === 'dm') saveLastView(app.meId, location.hash)
  })

  // Espace réservé aux admins : retour à l'accueil sinon (le serveur refuse de toute façon).
  $effect(() => { if (route.name === 'admin' && !app.isAdmin) router.go('#/') })
  $effect(() => { if (route.name === 'films' && !app.movies) router.go('#/') })
  const space = $derived(route.name === 'space' ? platform.spaces.find((s) => s.id === route.space) : undefined)
  $effect(() => { if (route.name === 'space' && !space?.visible()) router.go('#/') })

  // Serveur inconnu (supprimé, quitté) : retour à l'accueil.
  $effect(() => {
    if (route.name === 'channel' && !app.details[route.serverId] && app.phase === 'ready') {
      const known = app.servers.some((s) => s.id === route.serverId)
      if (!known) { router.go('#/'); notify.info("Ce serveur n'existe plus ou n'est plus accessible.") }
    }
  })
</script>

<svelte:window onkeydown={(e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); openSearch() }
  if (platform.reportShortcut && e.ctrlKey && e.altKey && e.code === 'KeyB') { e.preventDefault(); void openReport() }
}} />
<a class="skip-link" href="#main" onclick={(e) => { e.preventDefault(); document.getElementById("main")?.focus() }}>Aller au contenu</a>
<div class="app" data-nav={nav ? 'open' : 'closed'}>
  <div class="nav-drawer" role="presentation" onclick={(e) => { if ((e.target as HTMLElement).closest('a')) nav = false }}>
    <ServerRail onadd={() => (modal = 'create')} />
    <SidePanel
      {serverId}
      activeChannel={channel?.id}
      onnewdm={() => (modal = 'dm')}
      onsettings={() => (modal = 'settings')}
      onserver={() => (modal = 'server')}
    />
  </div>
  {#if app.on('musique')}<MusicDock />{/if}
  <SoloDock />
  <button class="scrim" aria-label="Fermer le menu" onclick={() => (nav = false)}></button>

  <main class="main" id="main" tabindex="-1">
    {#if app.connection === 'closed' || app.connection === 'connecting'}
      <div class="conn-banner" role="status">Reconnexion en cours…</div>
    {/if}
    {#if route.name === 'admin' && app.isAdmin}
      <AdminSpace section={route.section} onmenu={() => (nav = true)} />
    {:else if route.name === 'films' && app.movies}
      <FilmsSpace section={route.section} id={route.id} onmenu={() => (nav = true)} />
    {:else if route.name === 'space' && space?.visible()}
      <space.page section={route.section} onmenu={() => (nav = true)} />
    {:else if route.name === 'join'}
      <JoinView code={route.code} />
    {:else if convKey}
      {#key convKey}
        <ChatView
          {convKey}
          title={route.name === 'dm' ? (app.dms.find((d) => d.id === route.id)?.other.pseudo ?? '…') : (channel?.name ?? '')}
          isDM={route.name === 'dm'}
          onmenu={() => (nav = true)}
        />
      {/key}
    {:else if channel && isVoiceKind(channel.kind)}
      <VoiceChannelView {channel} onmenu={() => (nav = true)} />
    {:else if channel?.kind === 'videos' || channel?.kind === 'music'}
      {#key channel.id}<VideosView {channel} {serverId} onmenu={() => (nav = true)} />{/key}
    {:else}
      <HomeView onmenu={() => (nav = true)} oncreate={() => (modal = 'create')} />
    {/if}
  </main>

</div>

{#if searchNav.open}<SearchModal onclose={() => (searchNav.open = false)} />{/if}
{#if modal === 'create'}<CreateJoinModal onclose={() => (modal = '')} />{/if}
{#if modal === 'dm'}<NewDMModal onclose={() => (modal = '')} />{/if}
{#if modal === 'settings'}<SettingsModal onclose={() => (modal = '')} />{/if}
{#if reportNav.open}<ReportModal onclose={() => (reportNav.open = false)} />{/if}
{#if profileNav.open}<ProfileModal {...profileNav.open} onclose={() => (profileNav.open = null)} />{/if}
{#if channelCreate.at}<CreateChannelModal {...channelCreate.at} onclose={() => (channelCreate.at = null)} />{/if}
{#if channelCreate.done}
  <div class="cc-done" role="status">
    <span class="truncate">Salon « {channelCreate.done.channel.name} » créé</span>
    <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => { channelCreate.done = null; modal = 'server' }}>Réglages</button>
  </div>
{/if}
{#if channelMod.at}<ChannelModModal {...channelMod.at} onclose={() => (channelMod.at = null)} />{/if}
{#if moderation.at}<ModerationModal {...moderation.at} onclose={() => (moderation.at = null)} />{/if}
{#if modal === 'server' && detail}<ServerSettingsModal {detail} onclose={() => (modal = '')} />{/if}
