<script lang="ts">
  import Loader from './Loader.svelte'
  import { untrack } from 'svelte'
  import Icon, { type IconName } from './Icon.svelte'
  import Modal from './Modal.svelte'
  import CreateJoinModal from './CreateJoinModal.svelte'
  import NewDMModal from './NewDMModal.svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { hrefs, router } from '../lib/route.svelte'
  import { searchNav } from '../lib/search.svelte'
  import { clearRecentSearches, excerpt, fold, highlightTerms, recentSearches, rememberSearch, suggestions, type Scope } from '../lib/search'
  import { dayOf, timeOf } from '../lib/format'
  import { notify } from '../lib/ui'
  import { platform } from '../lib/platform'
  import { openSettings, SETTINGS_TABS } from '../lib/settings.svelte'
  import { openProfile } from '../lib/profile.svelte'
  import { openReport } from '../blocks/signalements/report.svelte'
  import { voice } from '../blocks/vocal/voice.svelte'
  import { isVoiceKind, type ChannelKind, type Message, type Status } from '../lib/types'

  let { onclose }: { onclose: () => void } = $props()

  const { convKey, serverId } = untrack(() => ({ convKey: searchNav.convKey, serverId: searchNav.serverId }))
  const inDM = convKey.startsWith('d:')
  const channelName = convKey.startsWith('c:') ? app.channel(convKey.slice(2))?.channel.name : undefined
  const dmName = inDM ? app.dms.find((d) => d.id === convKey.slice(2))?.other.pseudo : undefined
  const serverName = serverId ? app.servers.find((s) => s.id === serverId)?.name : undefined

  const scopes = $derived<[Scope, string, string][]>([
    ['all', 'Partout', 'Partout'],
    ...(serverName ? [['server', `Ce serveur (${serverName})`, serverName] as [Scope, string, string]] : []),
    ...(convKey ? [['conv', inDM ? `Cette conversation (${dmName ?? ''})` : `Ce salon (#${channelName ?? ''})`, inDM ? `@${dmName ?? ''}` : `#${channelName ?? ''}`] as [Scope, string, string]] : []),
  ])

  let query = $state('')
  let scope = $state<Scope>(convKey ? 'conv' : 'all')
  let sort = $state<'relevance' | 'date'>('relevance')
  let results = $state<Message[]>([])
  let more = $state(false)
  let busy = $state(false)
  let searched = $state('')
  let recents = $state(recentSearches())
  let active = $state(-1)
  let sentinel = $state<HTMLElement>()
  let sous = $state<'' | 'create' | 'dm'>('')
  let largeur = $state(1024)
  let seq = 0

  const scopeParams = $derived(
    scope === 'server' ? { server_id: serverId } : scope === 'conv' ? (inDM ? { dm_id: convKey.slice(2) } : { channel_id: convKey.slice(2) }) : {},
  )
  const hints = $derived(suggestions(query, {
    pseudos: [...new Set(scope === 'all' ? app.servers.flatMap((s) => app.details[s.id]?.members.map((m) => m.pseudo) ?? []).concat(app.dms.map((d) => d.other.pseudo)) : app.pseudosOf(convKey))],
    channels: [...new Set((scope === 'all' ? app.servers : app.servers.filter((s) => s.id === serverId)).flatMap((s) => app.details[s.id]?.channels.filter((c) => c.kind === 'text').map((c) => c.name) ?? []))],
    servers: app.servers.map((s) => s.name),
  }))
  const terms = $derived(highlightTerms(searched))

  interface Entree { id: string; groupe: 'filtre' | 'aller' | 'action'; icone: IconName; label: string; detail?: string; kbd?: string; run: () => void }

  const KIND_ICON: Record<ChannelKind, IconName> = { text: 'hash', voice: 'vol', cinema: 'film', videos: 'video', music: 'casque' }
  const STATUTS: [Status, string][] = [['online', 'En ligne'], ['idle', 'Absent'], ['dnd', 'Ne pas déranger']]

  function go(href: string) {
    onclose()
    router.go(href)
  }
  function puis(f: () => unknown) {
    onclose()
    void f()
  }

  const saisie = $derived(query.trimStart())
  const prefixe = $derived(['>', '@', '#'].includes(saisie[0]) ? saisie[0] : '')
  const texte = $derived(fold((prefixe ? saisie.slice(1) : saisie).trim()))
  const libre = $derived(!/\S:\S|\S:$/.test(query))

  const destinations = $derived.by<Entree[]>(() => {
    const out: Entree[] = []
    if (prefixe !== '#' && prefixe !== '@') {
      out.push({ id: 'accueil', groupe: 'aller', icone: 'maison', label: 'Accueil', run: () => go(hrefs.home()) })
      for (const s of app.servers) out.push({ id: `s:${s.id}`, groupe: 'aller', icone: 'calque', label: s.name, detail: 'Serveur', run: () => go(hrefs.channel(s.id)) })
      if (app.movies) out.push({ id: 'films', groupe: 'aller', icone: 'film', label: 'Films et séries', run: () => go(hrefs.films()) })
      for (const sp of platform.spaces) if (sp.visible()) out.push({ id: `e:${sp.id}`, groupe: 'aller', icone: sp.icon, label: sp.title, run: () => go(hrefs.space(sp.id)) })
      if (app.adminSections.length) out.push({ id: 'admin', groupe: 'aller', icone: 'bouclier', label: 'Administration', run: () => go(hrefs.admin()) })
    }
    if (prefixe !== '@') {
      for (const s of app.servers) {
        for (const c of app.details[s.id]?.channels ?? []) {
          out.push({
            id: `c:${c.id}`, groupe: 'aller', icone: KIND_ICON[c.kind] ?? 'hash', label: c.name, detail: s.name,
            run: () => {
              go(hrefs.channel(s.id, c.id))
              if (isVoiceKind(c.kind) && app.can(s.id, 'connect', c.id)) void voice.join(c.id)
            },
          })
        }
      }
    }
    if (prefixe !== '#') {
      for (const d of app.dms) out.push({ id: `d:${d.id}`, groupe: 'aller', icone: 'arobase', label: d.other.pseudo, detail: 'Message privé', run: () => go(hrefs.dm(d.id)) })
    }
    return out
  })

  const salonVocal = $derived.by(() => {
    const r = router.current
    if (r.name !== 'channel' || !r.channelId) return null
    const c = app.channel(r.channelId)
    if (!c || !isVoiceKind(c.channel.kind) || !app.can(c.server.id, 'connect', c.channel.id)) return null
    if (voice.channelId === c.channel.id && voice.state !== 'idle') return null
    return c.channel
  })

  const actions = $derived.by<Entree[]>(() => [
    { id: 'dm', groupe: 'action', icone: 'bulle', label: 'Nouveau message privé', run: () => (sous = 'dm') },
    { id: 'serveur', groupe: 'action', icone: 'plus', label: 'Créer ou rejoindre un serveur', run: () => (sous = 'create') },
    ...(salonVocal ? [{ id: 'vocal', groupe: 'action', icone: 'vol', label: `Rejoindre « ${salonVocal.name} »`, run: () => puis(() => voice.join(salonVocal.id)) } satisfies Entree] : []),
    ...(voice.state !== 'idle' ? [
      { id: 'micro', groupe: 'action', icone: voice.muted ? 'mic' : 'micoff', label: voice.muted ? 'Réactiver le micro' : 'Couper le micro', kbd: 'Alt Maj M', run: () => puis(() => voice.toggleMute()) },
      { id: 'casque', groupe: 'action', icone: voice.deafened ? 'casque' : 'casqueoff', label: voice.deafened ? 'Réactiver le son' : 'Sourdine casque', kbd: 'Alt Maj D', run: () => puis(() => voice.toggleDeafen()) },
      { id: 'quitter', groupe: 'action', icone: 'raccroche', label: 'Quitter le vocal', run: () => puis(() => voice.leave()) },
    ] satisfies Entree[] : []),
    { id: 'bug', groupe: 'action', icone: 'bug', label: 'Signaler un bug', kbd: platform.reportShortcut ? 'Ctrl Alt B' : undefined, run: () => puis(() => openReport()) },
    ...STATUTS.map(([st, label]): Entree => ({
      id: `statut:${st}`, groupe: 'action', icone: 'perso', label: `Statut : ${label}`, detail: (app.statuses[app.meId] ?? 'online') === st ? 'actuel' : undefined,
      run: () => puis(() => app.setStatus(st).catch((err) => notify.error(err, 'Changement de statut impossible'))),
    })),
    { id: 'profil', groupe: 'action', icone: 'perso', label: 'Voir mon profil', run: () => puis(() => openProfile(app.meId, serverId)) },
    ...SETTINGS_TABS.map((t): Entree => ({ id: `param:${t.id}`, groupe: 'action', icone: 'reglages', label: `Paramètres › ${t.label}`, run: () => puis(() => openSettings(t.id)) })),
  ])

  const correspond = (e: Entree) => fold(e.label).includes(texte) || (!!e.detail && fold(e.detail).includes(texte))

  const aller = $derived(libre && prefixe !== '>' && (texte || prefixe) ? destinations.filter(correspond).slice(0, texte ? 8 : 50) : [])
  const faire = $derived(!libre || prefixe === '@' || prefixe === '#' ? [] : texte ? actions.filter(correspond).slice(0, 8) : prefixe === '>' ? actions : !query.trim() ? actions.slice(0, 6) : [])
  const filtres = $derived(hints.map((s): Entree => ({ id: `f:${s.insert}`, groupe: 'filtre', icone: 'filtre', label: s.label, run: () => apply(s) })))
  const entrees = $derived([...filtres, ...aller, ...faire])
  const sections = $derived(([['filtre', 'Filtres'], ['aller', 'Aller à'], ['action', 'Actions']] as const)
    .map(([g, titre]) => ({ g, titre, items: entrees.map((e, i) => ({ e, i })).filter((x) => x.e.groupe === g) }))
    .filter((s) => s.items.length))

  async function run(older = false) {
    const q = query.trim()
    if (!q || busy && older) return
    const mine = ++seq
    busy = true
    try {
      const page = await api.search(q, scopeParams, sort, older ? results.length : 0)
      if (mine !== seq) return
      results = older ? [...results, ...page.messages] : page.messages
      more = page.has_more
      searched = q
      if (!older) recents = rememberSearch(q)
    } catch (e) {
      if (mine === seq) notify.error(e, 'Recherche impossible')
    } finally {
      if (mine === seq) busy = false
    }
  }

  // Changer de portée ou de tri relance la recherche en cours.
  let first = true
  $effect(() => {
    void scope, sort
    if (first) { first = false; return }
    if (searched) untrack(() => void run())
  })

  // Pagination au défilement : la sentinelle de fin de liste charge la page suivante.
  $effect(() => {
    if (!sentinel || !more) return
    const o = new IntersectionObserver((e) => { if (e[0].isIntersecting && !busy) void run(true) }, { rootMargin: '200px' })
    o.observe(sentinel)
    return () => o.disconnect()
  })

  $effect(() => {
    if (active < 0) return
    document.getElementById(`palette-${active}`)?.scrollIntoView({ block: 'nearest' })
  })

  function apply(s: { insert: string }) {
    query = s.insert
    active = -1
    document.getElementById('search-input')?.focus()
  }

  function onkeydown(e: KeyboardEvent) {
    const n = entrees.length
    if (n && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault()
      active = e.key === 'ArrowDown' ? (active + 1) % n : active <= 0 ? n - 1 : active - 1
    } else if (e.key === 'Tab' && !e.shiftKey && active >= 0 && entrees[active]?.groupe === 'filtre') {
      e.preventDefault()
      entrees[active].run()
    } else if (e.key === 'Enter') {
      e.preventDefault()
      if (active >= 0 && entrees[active]) entrees[active].run()
      else void run()
    }
  }

  /** Salon (avec son serveur) ou conversation privée d'un message : titre de groupe et adresse. */
  function where(m: Message): { group: string; href: string } {
    if (m.dm_id) return { group: `@ ${app.dms.find((d) => d.id === m.dm_id)?.other.pseudo ?? 'Message privé'}`, href: hrefs.dm(m.dm_id) }
    const c = app.channel(m.channel_id!)
    return { group: c ? `${c.server.name} › # ${c.channel.name}` : 'Salon', href: c ? hrefs.channel(c.server.id, c.channel.id) : '' }
  }

  const groups = $derived.by(() => {
    const out: { group: string; items: { m: Message; href: string }[] }[] = []
    for (const m of results) {
      const w = where(m)
      let g = out.find((x) => x.group === w.group)
      if (!g) out.push((g = { group: w.group, items: [] }))
      g.items.push({ m, href: w.href })
    }
    return out
  })

  function open(m: Message, href: string) {
    if (!href) return
    onclose()
    router.go(`${href}/${m.id}`)
  }
</script>

<svelte:window bind:innerWidth={largeur} />

{#if sous === 'create'}
  <CreateJoinModal {onclose} />
{:else if sous === 'dm'}
  <NewDMModal {onclose} />
{:else}
  <Modal title="Rechercher" wide {onclose}>
    <div class="palette">
      <form class="barre" onsubmit={(e) => { e.preventDefault(); void run() }}>
        <span class="barre-icone" aria-hidden="true"><Icon name="cherche" size={20} /></span>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="search-input" class="champ" type="search" placeholder={largeur > 700 ? 'Aller à, agir ou chercher : « expression », -exclu, de:pseudo…' : 'Aller à, agir ou chercher…'} bind:value={query} {onkeydown} oninput={() => (active = -1)} maxlength="200" autofocus autocomplete="off" aria-label="Rechercher" aria-controls={sections.length ? 'palette-liste' : undefined} aria-activedescendant={active >= 0 ? `palette-${active}` : undefined} {@attach (n) => { const t = setTimeout(() => n.focus()); return () => clearTimeout(t) }} />
        {#if query}<button type="button" class="v-ib sm effacer" aria-label="Effacer la saisie" onclick={() => { query = ''; active = -1; document.getElementById('search-input')?.focus() }}><Icon name="x" size={16} /></button>{/if}
        <button class="plbls-button plbls-button--compact" class:is-busy={busy} disabled={!query.trim()}>Chercher</button>
      </form>
      <div class="opts">
        <div class="v-seg" role="group" aria-label="Où">
          {#each scopes as [v, label, court] (v)}<button type="button" aria-pressed={scope === v} aria-label={label} title={label} onclick={() => (scope = v)}><span class="v-ell">{court}</span></button>{/each}
        </div>
        <span class="grow"></span>
        <span class="v-note" aria-hidden="true">Tri</span>
        <div class="v-seg" role="group" aria-label="Tri">
          <button type="button" aria-pressed={sort === 'relevance'} onclick={() => (sort = 'relevance')}>Pertinence</button>
          <button type="button" aria-pressed={sort === 'date'} onclick={() => (sort = 'date')}>Plus récents</button>
        </div>
      </div>
      <div class="corps">
        {#if sections.length}
        <div id="palette-liste" role="listbox" aria-label="Suggestions">
          {#each sections as s (s.g)}
            <div role="group" aria-labelledby="palette-g-{s.g}">
              <div class="v-mh" id="palette-g-{s.g}">{s.titre}</div>
              {#each s.items as { e, i } (e.id)}
                <div class="v-mi entree" class:on={i === active} id="palette-{i}" role="option" tabindex="-1" aria-selected={i === active} onclick={() => e.run()} onkeydown={(ev) => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); e.run() } }} onpointermove={() => (active = i)}>
                  <Icon name={e.icone} size={16} />
                  <span class="v-ell entree-label">{e.label}</span>
                  {#if e.detail}<span class="v-mute entree-detail v-ell">{e.detail}</span>{/if}
                  {#if e.kbd}<span class="v-kbd">{e.kbd}</span>{:else if i === active}<span class="v-kbd">Entrée</span>{/if}
                </div>
              {/each}
            </div>
          {/each}
        </div>
        {/if}
        <div class="resultats" role="region" aria-label="Résultats" aria-live="polite">
          {#if groups.length}<div class="v-mh">Messages{more ? '' : ` · ${results.length} résultat${results.length > 1 ? 's' : ''}`}</div>{/if}
          {#each groups as g (g.group)}
            <h3 class="groupe">{g.group}</h3>
            {#each g.items as { m, href } (m.id)}
              <button class="hit" onclick={() => open(m, href)}>
                <span class="hit-tete"><b>{m.author.pseudo}</b><span class="v-mono v-mute2">{dayOf(m.created_at)} {timeOf(m.created_at)}</span></span>
                <span class="hit-texte">{#each excerpt(m.content, terms) as p}{#if p.hit}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}</span>
                {#if m.attachments.length}<span class="v-note hit-pj"><Icon name="trombone" size={14} /> {m.attachments.length} fichier{m.attachments.length > 1 ? 's' : ''}</span>{/if}
              </button>
            {/each}
          {/each}
          {#if more}<div bind:this={sentinel} class="suite"><Loader /></div>{/if}
          {#if !results.length && !busy}
            {#if searched}
              <div class="vide"><Icon name="cherche" size={28} /><b>Aucun résultat pour « {searched} ».</b>{#if scope !== 'all'}<span class="v-note">Essaie d'élargir à « Partout ».</span>{/if}</div>
            {:else if recents.length}
              <div class="recents-tete"><span class="v-mh grow">Dernières recherches</span><button class="v-btn sm ghost" onclick={() => { clearRecentSearches(); recents = [] }}>Effacer</button></div>
              {#each recents as r (r)}<button class="v-mi recent" onclick={() => { query = r; void run() }}><Icon name="horloge" size={14} /><span class="v-ell">{r}</span></button>{/each}
            {:else}
              <p class="v-note aide">Filtres : de:pseudo · dans:#salon · serveur:nom · mentionne:pseudo · avant:/après:/pendant:AAAA-MM-JJ · a:fichier|image|vidéo|lien|sondage · épinglé:oui</p>
            {/if}
          {/if}
        </div>
      </div>
      <div class="pied v-note" aria-hidden="true">
        <span><span class="v-kbd">↑</span> <span class="v-kbd">↓</span> parcourir</span>
        <span><span class="v-kbd">Entrée</span> ouvrir, ou chercher dans les messages</span>
        <span><span class="v-kbd">Tab</span> compléter un filtre</span>
        <span><span class="v-kbd">Échap</span> fermer</span>
        <span class="grow"></span>
        <span class="v-mono">&gt; actions · @ personnes · # salons</span>
      </div>
    </div>
  </Modal>
{/if}

<style>
  :global(plbls-modal:has(.palette) .plbls-modal__entete) { padding: 14px 16px 0 20px; align-items: center; }
  :global(plbls-modal:has(.palette) .plbls-modal__entete > h3) { font-size: 15px; color: var(--gris); }
  :global(plbls-modal:has(.palette) .plbls-modal__corps) { padding: 0; }
  :global(plbls-modal:has(.palette) .plbls-modal) { overflow: hidden; }
  .palette { height: 100%; min-height: 0; display: grid; grid-template-rows: auto auto minmax(0, 1fr) auto; }
  .barre { display: flex; align-items: center; gap: 10px; padding: 10px 16px 12px; border-bottom: 1px solid var(--trait); }
  .barre-icone { display: inline-flex; color: var(--gris); }
  .champ { flex: 1; min-width: 0; height: 40px; padding: 0 4px; border: 0; border-radius: var(--r-xs); background: transparent; color: var(--encre); font: inherit; font-size: 17px; }
  .champ::placeholder { color: var(--gris-2); text-overflow: ellipsis; }
  .champ::-webkit-search-cancel-button, .champ::-webkit-search-decoration { -webkit-appearance: none; appearance: none; }
  .effacer { flex: none; color: var(--gris); }
  .champ:focus-visible { outline: none; box-shadow: var(--halo); }
  .opts { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; padding: 10px 16px; border-bottom: 1px solid var(--trait); }
  .opts .v-seg { max-width: 100%; min-width: 0; }
  .opts .v-seg > button { min-width: 0; max-width: 220px; }
  .opts .v-seg > button:focus-visible { outline: none; box-shadow: var(--anneau-in); }
  .corps { min-height: 0; overflow: auto; padding: 8px; display: flex; flex-direction: column; gap: 2px; }
  .entree { height: auto; min-height: 36px; padding-block: 6px; }
  .entree-label { min-width: 0; }
  .entree-detail { flex: 0 1 auto; font-size: 12px; }
  .entree :global(svg) { flex: none; }
  .resultats { display: flex; flex-direction: column; gap: 2px; }
  .groupe { margin: 8px 10px 2px; font-size: 12px; font-weight: 600; color: var(--gris); }
  .hit { display: flex; flex-direction: column; gap: 2px; width: 100%; padding: 10px; border: 0; border-radius: var(--r-s); background: transparent; color: var(--encre); font: inherit; font-size: 14px; text-align: left; cursor: pointer; }
  .hit:hover { background: var(--rose-doux); }
  .hit:focus-visible { outline: none; box-shadow: var(--anneau); }
  .hit-tete { display: flex; align-items: baseline; gap: 8px; }
  .hit-texte { overflow-wrap: anywhere; white-space: pre-wrap; }
  .hit-pj { display: inline-flex; align-items: center; gap: 6px; }
  .hit mark { padding: 0 2px; border-radius: 3px; background: var(--rose); color: var(--encre); }
  .suite { display: grid; place-items: center; padding: 12px; }
  .vide { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 32px 16px; color: var(--gris); text-align: center; }
  .vide b { color: var(--encre); }
  .recents-tete { display: flex; align-items: center; gap: 8px; padding-right: 4px; }
  .recent { color: var(--gris); }
  .aide { margin: 8px 10px; }
  .pied { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 14px; padding: 10px 16px; border-top: 1px solid var(--trait); background: var(--surface-2); font-size: 12px; }
  @media (max-width: 700px) {
    .pied { display: none; }
    .barre .plbls-button { padding: 0 10px; }
  }
  @media (pointer: coarse) {
    .entree, .hit, .recent { min-height: 44px; }
    .opts .v-seg > button { height: 40px; }
  }
</style>
