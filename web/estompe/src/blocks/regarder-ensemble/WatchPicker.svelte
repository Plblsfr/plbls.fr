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
  <div class="wp-list">
    {#each rooms as r (r.id)}
      {@const here = people(r.id)}
      <button class="wp-room" class:here={r.id === connected} disabled={busy} onclick={() => void go(r)}>
        <Icon name={kind === 'cinema' ? 'film' : 'vol'} size={22} />
        <span class="wp-name">
          <strong class="truncate">{r.name}</strong>
          {#if !serverId}<span class="wp-sub truncate">{app.servers.find((x) => x.id === r.sid)?.name}</span>{/if}
          {#if current(r.id)}<span class="wp-sub truncate">Session en cours : {current(r.id)}</span>{/if}
        </span>
        {#if r.id === connected}<span class="v-tag rose">Vous y êtes</span>{/if}
        {#if here.length}
          <span class="wp-people" title="{here.length} participant{here.length > 1 ? 's' : ''}">
            {#each here.slice(0, 3) as p (p.user_id)}<Avatar pseudo={p.pseudo} userId={p.user_id} serverId={r.sid} size="sm" />{/each}
            {#if here.length > 3}<span class="v-mono v-mute">+{here.length - 3}</span>{/if}
          </span>
        {/if}
      </button>
    {:else}
      {@const room = kind === 'cinema' ? 'salon cinéma' : 'salon vocal'}
      <p class="v-empty v-note">{serverId ? `Aucun ${room} disponible sur ce serveur.` : `Aucun ${room} disponible.`}</p>
    {/each}
  </div>
</Modal>

<style>
  .wp-list { display: grid; gap: 6px; }
  .wp-room { display: flex; align-items: center; gap: 12px; width: 100%; min-height: 56px; padding: 12px; border: 0; border-radius: var(--r); background: transparent; color: var(--encre); font: inherit; text-align: left; cursor: pointer; transition: background var(--d) var(--sortie); }
  .wp-room > :global(.icon) { flex: none; color: var(--gris); }
  .wp-room:hover:not(:disabled) { background: var(--papier-2); }
  .wp-room.here { background: var(--rose-doux); }
  .wp-room:focus-visible { outline: none; box-shadow: var(--anneau); }
  .wp-room:disabled { opacity: 0.6; cursor: progress; }
  .wp-name { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .wp-sub { font-size: 12px; color: var(--gris); }
  .wp-people { display: inline-flex; align-items: center; gap: 2px; margin-left: auto; flex: none; }
</style>
