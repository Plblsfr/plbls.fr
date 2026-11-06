<script lang="ts">
  import Modal from '../../components/Modal.svelte'
  import Icon from '../../components/Icon.svelte'
  import Avatar from '../../components/Avatar.svelte'
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { startWatching } from '../videos/videos.svelte'
  import { notify } from '../../lib/ui'
  import { voice } from '../vocal/voice.svelte'
  import type { SessionKind } from '../../lib/types'
  import { autoRoom, roomKind } from './watch'

  let { serverId, links, kind = 'watch', position = 0, title, onclose }: { serverId?: string; links: string[]; kind?: SessionKind; position?: number; title?: string; onclose: () => void } = $props()

  const connected = $derived(voice.state !== 'idle' ? voice.channelId : null)
  const servers = $derived(serverId ? [serverId] : app.servers.map((s) => s.id))
  // le salon où l'on est connecté passe en premier
  const rooms = $derived(servers.flatMap((sid) => (app.details[sid]?.channels ?? []).filter((c) => c.kind === roomKind(kind) && app.can(sid, 'connect', c.id)).map((c) => ({ ...c, sid }))).sort((a, b) => +(b.id === connected) - +(a.id === connected)))
  const people = (id: string) => (app.voice[id] ?? []).filter((p) => !p.user_id.endsWith('#screen'))
  const current = (id: string) => { const s = app[kind][id]?.state; return s && (s.meta?.title || s.video) }
  let busy = $state(false)
  let autoTried = false

  async function go(room: { id: string; sid: string }) {
    busy = true
    try {
      await startWatching(room.sid, room.id, links, kind)
      if (position > 0) await api.watchAction(room.id, 'seek', { position }, kind)
      onclose()
    } catch (e) { notify.error(e, 'Impossible de lancer la session'); busy = false }
  }
  // Déjà dans un salon vocal de ce serveur, ou un seul salon possible : pas de question à poser.
  $effect(() => {
    const auto = autoRoom(rooms, connected)
    if (auto && !busy && !autoTried) { autoTried = true; void go(auto) }
  })
</script>

<Modal title={title ?? (kind === 'music' ? 'Écouter ensemble dans…' : kind === 'cinema' ? 'Regarder au cinéma dans…' : 'Regarder ensemble dans…')} {onclose}>
  <div class="stack">
    {#each rooms as r (r.id)}
      {@const here = people(r.id)}
      <button class="plbls-button plbls-button--contour wp-room" disabled={busy} onclick={() => void go(r)}>
        <Icon name={kind === 'cinema' ? 'film' : 'volume'} size={16} />
        <span class="wp-name">
          <span>{r.name}</span>
          {#if !serverId}<span class="small muted">{app.servers.find((x) => x.id === r.sid)?.name}</span>{/if}
          {#if current(r.id)}<span class="small muted">Session en cours : {current(r.id)}</span>{/if}
        </span>
        {#if r.id === connected}<span class="small muted">Vous y êtes</span>{/if}
        {#if here.length}
          <span class="wp-people" title="{here.length} participant{here.length > 1 ? 's' : ''}">
            {#each here.slice(0, 3) as p (p.user_id)}<Avatar pseudo={p.pseudo} userId={p.user_id} serverId={r.sid} size="sm" />{/each}
            {#if here.length > 3}<span class="small muted">+{here.length - 3}</span>{/if}
          </span>
        {/if}
      </button>
    {:else}
      {@const room = kind === 'cinema' ? 'salon cinéma' : 'salon vocal'}
      <p class="muted">{serverId ? `Aucun ${room} disponible sur ce serveur.` : `Aucun ${room} disponible.`}</p>
    {/each}
  </div>
</Modal>
