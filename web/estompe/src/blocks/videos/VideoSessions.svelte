<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import Icon from '../../components/Icon.svelte'
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { countRsvp, localInput, playable, sessionPhase, when } from './videos'
  import { joinSession } from './videos.svelte'
  import type { Rsvp, VideoList, VideoPost, WatchSession } from '../../lib/types'

  let { channelId, serverId, sessions, posts, lists, planList = '', onplanned }: {
    channelId: string; serverId: string; sessions: WatchSession[]; posts: VideoPost[]; lists: VideoList[]; planList?: string; onplanned: () => void
  } = $props()

  const fail = (e: unknown) => notify.error(e, 'Action impossible')
  const canPlan = $derived(app.can(serverId, 'organize_watch', channelId))
  const rooms = $derived((app.details[serverId]?.channels ?? []).filter((c) => c.kind === 'voice'))
  const roomName = (id: string) => rooms.find((r) => r.id === id)?.name ?? 'salon vocal'
  const who = (id: string) => app.details[serverId]?.members.find((m) => m.id === id)?.pseudo ?? '…'
  const byId = $derived(new Map(posts.map((p) => [p.message_id, p])))

  const RSVP: [Rsvp, string][] = [['yes', 'Je viens'], ['maybe', 'Peut-être'], ['no', 'Non']]

  // svelte-ignore state_referenced_locally
  let open = $state(!!planList)
  let title = $state('')
  let at = $state(localInput(new Date(Date.now() + 3600_000)))
  let room = $state('')
  // svelte-ignore state_referenced_locally
  let source = $state<string>(planList || '')
  let picked = $state<string[]>([])
  let remind = $state(10)
  $effect(() => { if (!room && rooms.length) room = rooms[0].id })
  $effect(() => { if (planList) { source = planList; open = true } })

  const ready = $derived(!!title.trim() && !!room && !!at && (source ? true : picked.length > 0))
  async function create(e: Event) {
    e.preventDefault()
    try {
      await api.sessionCreate(channelId, {
        title: title.trim(), starts_at: new Date(at).toISOString(), voice_channel_id: room, remind_minutes: remind,
        ...(source ? { list_id: source } : { message_ids: picked }),
      })
      title = ''; picked = []; open = false
      onplanned()
    } catch (err) { fail(err) }
  }
  const answer = (s: WatchSession, a: Rsvp) => void api.sessionRsvp(channelId, s.id, a).catch(fail)
  const cancel = async (s: WatchSession) => { if (await confirmer({ titre: 'Annuler la séance', message: `Annuler la séance « ${s.title} » ?`, action: 'Annuler la séance', destructif: true })) void api.sessionDelete(channelId, s.id).catch(fail) }
</script>

<div class="stack vsessions">
  {#if canPlan}
    <div class="row"><strong class="grow">Séances</strong><button class="plbls-button plbls-button--compact" onclick={() => (open = !open)} aria-expanded={open}>Planifier une séance</button></div>
    {#if open}
      <form class="stack vform" onsubmit={create}>
        <input class="plbls-input" bind:value={title} maxlength="60" placeholder="Titre (« Soirée bandes-annonces »)" aria-label="Titre de la séance" />
        <div class="row wrap">
          <label class="small stack"><span>Date et heure</span><input class="plbls-input" type="datetime-local" bind:value={at} /></label>
          <label class="small stack"><span>Salon vocal</span>
            <select class="plbls-select" bind:value={room}>{#each rooms as r (r.id)}<option value={r.id}>{r.name}</option>{/each}</select>
          </label>
          <label class="small stack"><span>Rappel</span>
            <select class="plbls-select" bind:value={remind}>
              <option value={0}>Aucun</option><option value={5}>5 min avant</option><option value={10}>10 min avant</option><option value={15}>15 min avant</option><option value={30}>30 min avant</option><option value={60}>1 h avant</option><option value={1440}>1 jour avant</option>
            </select>
          </label>
        </div>
        <label class="small stack"><span>File de la séance</span>
          <select class="plbls-select" bind:value={source}>
            <option value="">Choisir des vidéos…</option>
            {#each lists as l (l.id)}<option value={l.id}>Liste « {l.name} » ({l.items.length})</option>{/each}
          </select>
        </label>
        {#if !source}
          <ul class="vpick">
            {#each posts.filter(playable) as p (p.message_id)}
              <li><label class="row"><input type="checkbox" bind:group={picked} value={p.message_id} /><span class="truncate">{p.title || p.url}</span></label></li>
            {:else}<li class="muted small">Aucune vidéo lisible dans le salon.</li>{/each}
          </ul>
        {/if}
        <div class="row"><span class="grow"></span><button class="plbls-button" disabled={!ready}>Planifier</button></div>
      </form>
    {/if}
  {/if}
  {#each sessions as s (s.id)}
    {@const live = sessionPhase(s) === 'live'}
    {@const c = countRsvp(s)}
    {@const mine = s.rsvps[app.meId]}
    <section class="vsession" class:live>
      <header class="row">
        <Icon name="calendar" size={16} />
        <strong class="grow truncate">{s.title}</strong>
        <span class="small" class:muted={!live}>{live ? (s.started_at ? 'En cours' : 'À l’heure') : when(s.starts_at)}</span>
      </header>
      <div class="small muted">{new Date(s.starts_at).toLocaleString('fr-FR', { weekday: 'long', day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit' })} · {roomName(s.voice_channel_id)} · {s.videos.length} vidéo{s.videos.length > 1 ? 's' : ''}{s.remind_minutes ? ` · rappel ${s.remind_minutes} min avant` : ''}</div>
      <ul class="vq small">{#each s.videos as id (id)}<li class="truncate">{byId.get(id)?.title || byId.get(id)?.url || 'Vidéo supprimée'}</li>{/each}</ul>
      <div class="row wrap">
        {#each RSVP as [a, label] (a)}<button class="plbls-button plbls-button--compact" class:plbls-button--contour={mine !== a} aria-pressed={mine === a} onclick={() => answer(s, a)}>{label}{c[a] ? ` (${c[a]})` : ''}</button>{/each}
        <span class="grow"></span>
        {#if s.started_at || live}<button class="plbls-button plbls-button--compact" onclick={() => void joinSession(serverId, s.voice_channel_id).catch(fail)}><Icon name="play" size={14} /> Rejoindre</button>{/if}
        {#if s.created_by === app.meId || app.can(serverId, 'manage_messages', channelId)}<button class="ib sm" onclick={() => cancel(s)} aria-label="Annuler la séance" title="Annuler la séance"><Icon name="trash" size={16} /></button>{/if}
      </div>
      {#if Object.keys(s.rsvps).length}
        <div class="small muted">
          {#each [['yes', 'Viennent'], ['maybe', 'Peut-être'], ['no', 'Absents']] as [a, label] (a)}
            {@const names = Object.entries(s.rsvps).filter(([, v]) => v === a).map(([u]) => who(u))}
            {#if names.length}<div>{label} : {names.join(', ')}</div>{/if}
          {/each}
        </div>
      {/if}
    </section>
  {:else}
    <p class="muted">Aucune séance prévue.</p>
  {/each}
</div>
