<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import Icon from '../../components/Icon.svelte'
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { listPosts, moved, playable, playLink } from './videos'
  import { PROVIDER_LABEL } from '../regarder-ensemble/providers'
  import { solo } from './solo.svelte'
  import type { SessionKind, VideoList, VideoPost } from '../../lib/types'

  let { channelId, serverId, lists, posts, onwatch, onplan }: {
    channelId: string; serverId: string; lists: VideoList[]; posts: VideoPost[]
    onwatch: (links: string[], kind?: SessionKind) => void; onplan: (listId: string) => void
  } = $props()

  const fail = (e: unknown) => notify.error(e, 'Action impossible')
  const canEdit = $derived(app.can(serverId, 'send_messages', channelId))
  const canPlan = $derived(app.can(serverId, 'organize_watch', channelId))
  const canDelete = (l: VideoList) => l.created_by === app.meId || app.can(serverId, 'manage_messages', channelId)
  // liste réservée à son créateur : les autres la lisent sans la modifier
  const canEditList = (l: VideoList) => canEdit && (l.collaborative || canDelete(l))
  let name = $state('')
  let collaborative = $state(true)
  let renaming = $state<Record<string, string>>({})

  async function create(e: Event) {
    e.preventDefault()
    const n = name.trim()
    if (!n) return
    try { await api.videoListCreate(channelId, n, collaborative); name = '' } catch (err) { fail(err) }
  }
  const save = (l: VideoList, ids: string[]) => void api.videoListItems(channelId, l.id, ids).catch(fail)
  async function rename(l: VideoList) {
    const n = renaming[l.id]?.trim()
    delete renaming[l.id]
    if (n && n !== l.name) await api.videoListRename(channelId, l.id, n).catch(fail)
  }
  async function remove(l: VideoList) {
    if (await confirmer({ titre: 'Supprimer la liste', message: `Supprimer la liste « ${l.name} » ? Les vidéos restent dans le salon.`, action: 'Supprimer la liste', destructif: true })) await api.videoListDelete(channelId, l.id).catch(fail)
  }
</script>

<div class="stack vlists">
  {#if canEdit}
    <form class="row" onsubmit={create}>
      <input class="plbls-input grow" bind:value={name} maxlength="60" placeholder="Nouvelle liste (« À voir ce week-end »…)" aria-label="Nom de la nouvelle liste" />
      <label class="row small"><input type="checkbox" bind:checked={collaborative} /> Collaborative</label>
      <button class="plbls-button plbls-button--compact" disabled={!name.trim()}>Créer</button>
    </form>
  {/if}
  {#each lists as l (l.id)}
    {@const items = listPosts(l, posts)}
    {@const links = items.filter((p) => playable(p) && p.provider !== 'ytmusic').map(playLink)}
    {@const tunes = items.filter((p) => p.provider === 'ytmusic').map(playLink)}
    <section class="vlist">
      <header class="row">
        {#if l.id in renaming}
          <input class="plbls-input grow" bind:value={renaming[l.id]} maxlength="60" aria-label="Nouveau nom de la liste" onkeydown={(e) => { if (e.key === 'Enter') void rename(l); if (e.key === 'Escape') delete renaming[l.id] }} />
          <button class="plbls-button plbls-button--compact" onclick={() => void rename(l)}>OK</button>
        {:else}
          <strong class="grow truncate">{l.name}</strong><span class="small muted">{items.length} élément{items.length > 1 ? 's' : ''}</span>
          {#if links.length}<button class="plbls-button plbls-button--compact" onclick={() => onwatch(links)} title="La liste devient la file de la session"><Icon name="play" size={14} /> Regarder</button>{/if}
          {#if tunes.length}<button class="plbls-button plbls-button--compact" onclick={() => onwatch(tunes, 'music')} title="Les titres deviennent la file de la session Music2gether"><span aria-hidden="true">🎵</span> Écouter</button>{/if}
          {#if tunes.length}
            <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void solo.play(items.filter((p) => p.provider === 'ytmusic')).catch(fail)} title="Écouter la liste seul, sans session"><Icon name="headphones" size={14} /> Écouter seul</button>
            <button class="ib sm" onclick={() => void solo.play(items.filter((p) => p.provider === 'ytmusic'), { shuffle: true }).catch(fail)} aria-label="Écouter seul dans le désordre" title="Mélanger"><Icon name="shuffle" size={16} /></button>
          {/if}
          {#if canPlan && (links.length || tunes.length)}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => onplan(l.id)}><Icon name="calendar" size={14} /> Planifier</button>{/if}
          {#if canDelete(l)}<button class="ib sm" onclick={() => void api.videoListRename(channelId, l.id, l.name, !l.collaborative).catch(fail)} aria-pressed={l.collaborative} aria-label={l.collaborative ? 'Réserver la liste à son créateur' : 'Rendre la liste collaborative'} title={l.collaborative ? 'Collaborative : cliquer pour la réserver à son créateur' : 'Réservée à son créateur : cliquer pour la rendre collaborative'}><Icon name={l.collaborative ? 'users' : 'lock'} size={16} /></button>{/if}
          {#if canEditList(l)}<button class="ib sm" onclick={() => (renaming[l.id] = l.name)} aria-label="Renommer la liste" title="Renommer"><Icon name="edit" size={16} /></button>{/if}
          {#if canDelete(l)}<button class="ib sm" onclick={() => void remove(l)} aria-label="Supprimer la liste" title="Supprimer"><Icon name="trash" size={16} /></button>{/if}
        {/if}
      </header>
      <ol class="vlist-items">
        {#each items as p, i (p.message_id)}
          <li class="row">
            <span class="small muted vsrc-tag">{PROVIDER_LABEL[p.provider] ?? 'Lien'}</span>
            <a class="grow truncate" href={p.url} target="_blank" rel="noopener noreferrer">{p.title || p.url}</a>
            {#if canEditList(l)}
              <button class="ib sm" disabled={i === 0} aria-label="Monter" onclick={() => save(l, moved(items.map((x) => x.message_id), i, -1))}><span class="up"><Icon name="chevron" size={14} /></span></button>
              <button class="ib sm" disabled={i === items.length - 1} aria-label="Descendre" onclick={() => save(l, moved(items.map((x) => x.message_id), i, 1))}><span class="down"><Icon name="chevron" size={14} /></span></button>
              <button class="ib sm" aria-label="Retirer de la liste" onclick={() => save(l, items.filter((x) => x.message_id !== p.message_id).map((x) => x.message_id))}><Icon name="x" size={14} /></button>
            {/if}
          </li>
        {:else}
          <li class="muted small">Liste vide : ajoutez des vidéos depuis leur carte (bouton liste).</li>
        {/each}
      </ol>
    </section>
  {:else}
    <p class="muted">Aucune liste. Créez-en une pour rassembler des vidéos et en faire la file d’une séance.</p>
  {/each}
</div>
