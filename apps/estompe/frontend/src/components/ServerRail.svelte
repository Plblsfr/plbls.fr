<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { cairn } from '../blocks/cairn/cairn.svelte'
  import { platform } from '../lib/platform'
  import { prefs } from '../lib/prefs.svelte'
  import { voice } from '../blocks/vocal/voice.svelte'
  import { router, hrefs } from '../lib/route.svelte'
  import { channelKey } from '../lib/types'
  import { badge } from '../lib/format'
  import { openSearch } from '../lib/search.svelte'
  import { adjacent, moveServer, orderServers } from '../lib/serverorder'
  import Icon from './Icon.svelte'
  import ServerIcon from './ServerIcon.svelte'

  let { onadd }: { onadd: () => void } = $props()

  const route = $derived(router.current)
  const homeActive = $derived(route.name === 'home' || route.name === 'dm')
  const servers = $derived(orderServers(app.servers, prefs.serverOrder))
  const voiceServer = $derived(voice.channelId ? app.channel(voice.channelId)?.server.id : undefined)

  const dmMentions = $derived(app.mentions.filter((m) => m.dm_id).length)
  const dmUnread = $derived(app.dms.reduce((n, d) => n + (app.unread[`d:${d.id}`] ?? 0), 0))

  function serverUnread(id: string): number {
    return (app.details[id]?.channels ?? []).reduce((n, c) => n + (app.unread[channelKey(c.id)] ?? 0), 0)
  }

  let list: HTMLElement
  $effect(() => {
    route
    list.querySelector('.rail-item.active')?.scrollIntoView({ block: 'nearest' })
  })

  let dragged = $state('')
  function drop(to: string) {
    if (dragged && dragged !== to) {
      prefs.serverOrder = moveServer(servers.map((s) => s.id), dragged, to)
      prefs.save()
    }
    dragged = ''
  }

  const current = $derived(route.name === 'channel' ? route.serverId : homeActive ? '' : '\0')
  function step(e: KeyboardEvent) {
    if (!e.ctrlKey || !e.altKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return
    e.preventDefault()
    const next = adjacent(['', ...servers.map((s) => s.id)], current, e.key === 'ArrowUp' ? -1 : 1)
    router.go(next ? hrefs.channel(next) : hrefs.home())
  }
</script>

<svelte:window onkeydown={step} />

<nav class="rail" aria-label="Navigation principale">
  <a class="rail-item" class:active={homeActive} href={hrefs.home()} title="Messages privés" aria-label="Messages privés" aria-current={homeActive ? 'page' : undefined}>
    <span class="rail-pill"></span>
    <Icon name="mail" size={22} />
    {#if dmMentions}<span class="pip rail-badge" aria-label="{dmMentions} mentions">{dmMentions}</span>
    {:else if dmUnread}<span class="pip rail-badge" aria-label="{dmUnread} nouveaux messages">{badge(dmUnread)}</span>{/if}
  </a>
  <hr class="rail-sep" />
  <div class="rail-list" bind:this={list}>
    {#each servers as s (s.id)}
      {@const m = app.serverMentionCount(s.id)}
      {@const u = serverUnread(s.id)}
      {@const active = route.name === 'channel' && route.serverId === s.id}
      <a class="rail-item" class:active class:unread={u > 0 && !active} class:dragging={dragged === s.id} href={hrefs.channel(s.id)} title={s.name} aria-label={s.name} aria-current={active ? 'page' : undefined}
        draggable="true" ondragstart={(e) => { dragged = s.id; e.dataTransfer?.setData('text/plain', s.name) }} ondragend={() => (dragged = '')}
        ondragover={(e) => { if (dragged && dragged !== s.id) e.preventDefault() }} ondrop={(e) => { e.preventDefault(); drop(s.id) }}>
        <span class="rail-pill"></span>
        <ServerIcon server={s} />
        {#if m}<span class="pip rail-badge" aria-label="{m} mentions">{m}</span>{/if}
        {#if voiceServer === s.id}<span class="rail-voice" role="img" aria-label="Connecté en vocal"><Icon name="volume" size={12} /></span>{/if}
      </a>
    {/each}
    <button class="rail-item rail-add" onclick={onadd} title="Créer ou rejoindre un serveur" aria-label="Créer ou rejoindre un serveur"><Icon name="plus" size={22} /></button>
  </div>
  <hr class="rail-sep" />
  <div class="rail-tools">
    <button class="rail-item" onclick={openSearch} title="Rechercher (Ctrl+K)" aria-label="Recherche globale"><Icon name="search" size={22} /></button>
    {#if cairn.visible}
      <a class="rail-item" class:active={route.name === 'cairn'} href={hrefs.cairn()} title="Tickets et documentation (Cairn)" aria-label="Cairn : tickets et documentation" aria-current={route.name === 'cairn' ? 'page' : undefined}>
        <span class="rail-pill"></span><Icon name="file" size={22} />
      </a>
    {/if}
    {#if app.movies}
      <a class="rail-item" class:active={route.name === 'films'} href={hrefs.films()} title="Films et séries" aria-label="Films et séries" aria-current={route.name === 'films' ? 'page' : undefined}>
        <span class="rail-pill"></span><Icon name="clap" size={22} />
      </a>
    {/if}
    {#each platform.spaces.filter((s) => s.visible()) as s (s.id)}
      <a class="rail-item" class:active={route.name === 'space' && route.space === s.id} href={hrefs.space(s.id)} title={s.title} aria-label={s.title} aria-current={route.name === 'space' && route.space === s.id ? 'page' : undefined}>
        <span class="rail-pill"></span><Icon name={s.icon} size={22} />
      </a>
    {/each}
    {#if app.isAdmin}
      <a class="rail-item" class:active={route.name === 'admin'} href={hrefs.admin()} title="Administration d'Estompe" aria-label="Administration d'Estompe" aria-current={route.name === 'admin' ? 'page' : undefined}>
        <span class="rail-pill"></span><Icon name="sliders" size={22} />
      </a>
    {/if}
  </div>
</nav>
