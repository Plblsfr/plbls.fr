<script lang="ts">
  import { entracteOn } from '../lib/api'
  import { app } from '../lib/store.svelte'
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
  let over = $state('')
  function drop(to: string) {
    if (dragged && dragged !== to) {
      prefs.serverOrder = moveServer(servers.map((s) => s.id), dragged, to)
      prefs.save()
    }
    dragged = ''
    over = ''
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
  <a class="rail-item rail-home" class:active={homeActive} class:unread={dmUnread > 0 && !homeActive} href={hrefs.home()} title="Messages privés" aria-current={homeActive ? 'page' : undefined}>
    <span class="rail-logo" aria-hidden="true">E</span><span class="v-sr">Messages privés</span>
    {#if dmMentions}<span class="pip rail-badge" aria-label="{dmMentions} mentions">{dmMentions}</span>
    {:else if dmUnread}<span class="pip pip-nb rail-badge" aria-label="{dmUnread} nouveaux messages">{badge(dmUnread)}</span>{/if}
  </a>
  <hr class="rail-sep" />
  <div class="rail-list" bind:this={list}>
    {#each servers as s (s.id)}
      {@const m = app.serverMentionCount(s.id)}
      {@const u = serverUnread(s.id)}
      {@const active = route.name === 'channel' && route.serverId === s.id}
      <a class="rail-item" class:active class:unread={u > 0 && !active} class:dragging={dragged === s.id} class:over={over === s.id} href={hrefs.channel(s.id)} title={s.name} aria-current={active ? 'page' : undefined}
        draggable="true" ondragstart={(e) => { dragged = s.id; e.dataTransfer?.setData('text/plain', s.name) }} ondragend={() => { dragged = ''; over = '' }}
        ondragover={(e) => { if (dragged && dragged !== s.id) { e.preventDefault(); over = s.id } }} ondragleave={() => { if (over === s.id) over = '' }} ondrop={(e) => { e.preventDefault(); drop(s.id) }}>
        <ServerIcon server={s} /><span class="v-sr">{s.name}</span>
        {#if m}<span class="pip rail-badge" aria-label="{m} mentions">{m}</span>{/if}
        {#if voiceServer === s.id}<span class="rail-voice" role="img" aria-label="Connecté en vocal" title="Connecté en vocal"><Icon name="onde" size={12} /></span>{/if}
      </a>
    {/each}
    <button class="rail-item rail-add" onclick={onadd} title="Créer ou rejoindre un serveur" aria-label="Créer ou rejoindre un serveur"><Icon name="plus" size={22} /></button>
  </div>
  <div class="rail-tools">
    <button class="rail-tool" onclick={openSearch} title="Rechercher (Ctrl+K)" aria-label="Recherche globale"><Icon name="cherche" size={22} /></button>
    {#if app.movies}
      <a class="rail-tool" class:active={route.name === 'films'} href={hrefs.films()} title={entracteOn() ? 'Entracte · films et séries' : 'Films et séries'} aria-label={entracteOn() ? 'Entracte, films et séries' : 'Films et séries'} aria-current={route.name === 'films' ? 'page' : undefined}><Icon name="film" size={22} /></a>
    {/if}
    {#each platform.spaces.filter((s) => s.visible()) as s (s.id)}
      <a class="rail-tool" class:active={route.name === 'space' && route.space === s.id} href={hrefs.space(s.id)} title={s.title} aria-label={s.title} aria-current={route.name === 'space' && route.space === s.id ? 'page' : undefined}><Icon name={s.icon} size={22} /></a>
    {/each}
    {#if app.adminSections.length}
      <a class="rail-tool" class:active={route.name === 'admin'} href={hrefs.admin()} title="Administration d'Estompe" aria-label="Administration d'Estompe" aria-current={route.name === 'admin' ? 'page' : undefined}><Icon name="bouclier" size={22} /></a>
    {/if}
  </div>
</nav>

<style>
  .rail { flex: none; width: 68px; display: flex; flex-direction: column; align-items: center; gap: var(--espace-2); min-height: 0; padding: 14px 0 var(--espace-3); background: var(--papier); box-shadow: inset -1px 0 0 var(--trait); }
  .rail-list { flex: 1; min-height: 0; width: 100%; display: flex; flex-direction: column; align-items: center; gap: 10px; overflow-y: auto; scrollbar-width: none; padding: 6px 0 10px; }
  .rail-tools { display: flex; flex-direction: column; align-items: center; gap: var(--espace-1); padding-top: var(--espace-2); }
  .rail-sep { flex: none; width: 28px; height: 1px; margin: 2px 0; border: 0; background: var(--trait-fort); }
  .rail-item { position: relative; flex: none; display: grid; place-items: center; width: 44px; height: 44px; padding: 0; border: 0; border-radius: var(--r); background: var(--surface); box-shadow: var(--o1); color: var(--encre); text-decoration: none; cursor: pointer; transition: box-shadow var(--d) var(--sortie), background var(--d) var(--sortie), transform var(--d) var(--ressort); }
  .rail-item:hover { background: var(--rose-doux); }
  .rail-item:active { transform: scale(0.96); }
  .rail-item:focus-visible, .rail-tool:focus-visible { outline: none; box-shadow: var(--anneau); }
  .rail-item.active { background: var(--rose); box-shadow: 0 0 0 2px var(--papier), 0 0 0 4px var(--encre); }
  .rail-item.unread::after { content: ''; position: absolute; left: 12px; right: 12px; bottom: -6px; height: 3px; border-radius: 3px; background: var(--rose-fonce); transform-origin: left; animation: v-trace var(--d-l) var(--sortie); }
  .rail-item.dragging { opacity: 0.4; }
  .rail-item.over { box-shadow: 0 -3px 0 0 var(--rose-fonce), var(--o1); }
  .rail-item :global(.sv-icon) { width: 100%; height: 100%; border-radius: inherit; background: transparent; color: inherit; font-family: var(--f-titre); font-weight: 800; font-size: 15px; }
  .rail-logo { font-family: var(--f-titre); font-weight: 800; font-size: 22px; line-height: 1; }
  .rail-home { background: var(--rose); box-shadow: none; }
  .rail-home:hover { background: var(--rose); box-shadow: 0 0 0 2px var(--papier), 0 0 0 4px var(--trait-fort); }
  .rail-home.active { box-shadow: 0 0 0 2px var(--papier), 0 0 0 4px var(--encre); }
  .rail-add { background: transparent; box-shadow: inset 0 0 0 1.5px var(--trait-fort); color: var(--gris); }
  .rail-add:hover { background: var(--papier-2); color: var(--encre); }
  .rail-badge { position: absolute; top: -6px; right: -8px; box-shadow: 0 0 0 3px var(--papier); }
  .rail-voice { position: absolute; right: -6px; bottom: -6px; display: grid; place-items: center; width: 20px; height: 20px; border-radius: 7px; background: var(--encre); color: var(--rose); box-shadow: 0 0 0 3px var(--papier); }
  .rail-tool { display: grid; place-items: center; width: 44px; height: 44px; border: 0; border-radius: var(--r-s); background: transparent; color: var(--gris); text-decoration: none; cursor: pointer; transition: background var(--d) var(--sortie), color var(--d) var(--sortie); }
  .rail-tool:hover { background: var(--papier-2); color: var(--encre); }
  .rail-tool.active { background: var(--rose); color: var(--encre); }
  @keyframes v-trace { from { transform: scaleX(0); } }
</style>
