<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import Icon from '../../components/Icon.svelte'
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { countRsvp, localInput, playable, sessionPhase, when } from './videos'
  import { joinSession } from './seance'
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
    <div class="vsess-head"><h2 class="v-eyebrow">Séances</h2><button class="plbls-button plbls-button--compact" onclick={() => (open = !open)} aria-expanded={open}><Icon name="plus" size={14} /> Planifier une séance</button></div>
    {#if open}
      <form class="stack vform" onsubmit={create}>
        <strong class="vform-title">Planifier une séance</strong>
        <label class="plbls-field"><span class="plbls-field__label">Titre</span><input class="plbls-input" bind:value={title} maxlength="60" placeholder="Titre (« Soirée bandes-annonces »)" aria-label="Titre de la séance" /></label>
        <div class="vform-grid">
          <label class="plbls-field"><span class="plbls-field__label">Date et heure</span><input class="plbls-input v-mono" type="datetime-local" bind:value={at} /></label>
          <label class="plbls-field"><span class="plbls-field__label">Salon vocal</span>
            <select class="plbls-select" bind:value={room}>{#each rooms as r (r.id)}<option value={r.id}>{r.name}</option>{/each}</select>
          </label>
          <label class="plbls-field"><span class="plbls-field__label">Rappel</span>
            <select class="plbls-select" bind:value={remind}>
              <option value={0}>Aucun</option><option value={5}>5 min avant</option><option value={10}>10 min avant</option><option value={15}>15 min avant</option><option value={30}>30 min avant</option><option value={60}>1 h avant</option><option value={1440}>1 jour avant</option>
            </select>
          </label>
        </div>
        <label class="plbls-field"><span class="plbls-field__label">File de la séance</span>
          <select class="plbls-select" bind:value={source}>
            <option value="">Choisir des vidéos…</option>
            {#each lists as l (l.id)}<option value={l.id}>Liste « {l.name} » ({l.items.length})</option>{/each}
          </select>
        </label>
        {#if !source}
          <ul class="vpick">
            {#each posts.filter(playable) as p (p.message_id)}
              <li><label class="row"><input type="checkbox" bind:group={picked} value={p.message_id} /><span class="truncate">{p.title || p.url}</span></label></li>
            {:else}<li class="v-note">Aucune vidéo lisible dans le salon.</li>{/each}
          </ul>
        {/if}
        <div class="vform-actions"><button class="plbls-button" disabled={!ready}>Planifier</button></div>
      </form>
    {/if}
  {/if}
  {#each sessions as s (s.id)}
    {@const live = sessionPhase(s) === 'live'}
    {@const c = countRsvp(s)}
    {@const mine = s.rsvps[app.meId]}
    {@const d = new Date(s.starts_at)}
    <section class="vsession" class:live>
      <header class="vsess-top">
        <span class="vsess-date" aria-hidden="true"><span class="v-mono">{d.toLocaleString('fr-FR', { month: 'short' }).replace('.', '').toUpperCase()}</span><b>{String(d.getDate()).padStart(2, '0')}</b></span>
        <span class="vsess-title"><strong class="truncate">{s.title}</strong>
      <span class="v-note">{new Date(s.starts_at).toLocaleString('fr-FR', { weekday: 'long', day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit' })} · {roomName(s.voice_channel_id)} · {s.videos.length} vidéo{s.videos.length > 1 ? 's' : ''}{s.remind_minutes ? ` · rappel ${s.remind_minutes} min avant` : ''}</span></span>
        <span class="v-tag" class:ok={live} class:att={!live}>{#if live}<span class="vsess-dot" aria-hidden="true"></span>{:else}<Icon name="horloge" size={12} />{/if}{live ? (s.started_at ? 'En cours' : 'À l’heure') : when(s.starts_at)}</span>
        {#if s.created_by === app.meId || app.can(serverId, 'manage_messages', channelId)}<button class="v-ib sm vdanger" onclick={() => cancel(s)} aria-label="Annuler la séance" title="Annuler la séance"><Icon name="corbeille" size={16} /></button>{/if}
      </header>
      <ul class="vq">{#each s.videos as id (id)}<li class="truncate">{byId.get(id)?.title || byId.get(id)?.url || 'Vidéo supprimée'}</li>{/each}</ul>
      <div class="vsess-rsvp">
        {#each RSVP as [a, label] (a)}<button class="v-chip" aria-pressed={mine === a} onclick={() => answer(s, a)}>{#if mine === a}<Icon name="ok" size={14} />{/if}{label}{c[a] ? ` (${c[a]})` : ''}</button>{/each}
        <span class="grow"></span>
        {#if s.started_at || live}<button class="plbls-button plbls-button--compact v-rose" onclick={() => void joinSession(serverId, s.voice_channel_id).catch(fail)}><Icon name="lecture" size={14} /> Rejoindre</button>{/if}
      </div>
      {#if Object.keys(s.rsvps).length}
        <div class="v-note">
          {#each [['yes', 'Viennent'], ['maybe', 'Peut-être'], ['no', 'Absents']] as [a, label] (a)}
            {@const names = Object.entries(s.rsvps).filter(([, v]) => v === a).map(([u]) => who(u))}
            {#if names.length}<div>{label} : {names.join(', ')}</div>{/if}
          {/each}
        </div>
      {/if}
    </section>
  {:else}
    <p class="v-empty v-note">Aucune séance prévue.</p>
  {/each}
</div>
