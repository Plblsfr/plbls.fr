<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { PROVIDER_LABEL } from '../regarder-ensemble/providers'
  import { playable, playLink, videoTitle } from './videos'
  import { videos } from './videos.svelte'
  import { voice } from '../vocal/voice.svelte'
  import { solo } from './solo.svelte'
  import { menuFrom } from '../../lib/ctxmenu.svelte'
  import type { SessionKind, VideoList, VideoPost } from '../../lib/types'

  let { post, channelId, serverId, lists, compact = false, onwatch }: {
    post: VideoPost; channelId: string; serverId: string; lists: VideoList[]; compact?: boolean; onwatch: (p: VideoPost) => void
  } = $props()

  const fail = (e: unknown) => notify.error(e, 'Action impossible')
  const mine = $derived(post.author.id === app.meId)
  const canOrganize = $derived(app.can(serverId, 'organize_watch', channelId) || app.can(serverId, 'manage_messages', channelId))
  const canRemove = $derived(mine || app.can(serverId, 'manage_messages', channelId))
  const canList = $derived(app.can(serverId, 'send_messages', channelId))
  const music = $derived(post.provider === 'ytmusic')
  const verb = $derived(music ? 'Écouter ensemble' : 'Regarder ensemble')
  // dans un salon vocal où une session est en cours : on peut y ajouter ce titre
  const kind = $derived<SessionKind>(music ? 'music' : 'watch')
  const session = $derived(voice.state !== 'idle' && voice.channelId && app[kind][voice.channelId] ? voice.channelId : '')

  let thumb = $state('')
  let box = $state<HTMLElement>()
  $effect(() => {
    if (!post.has_thumb || !box) return
    const id = post.message_id
    const io = new IntersectionObserver((es) => {
      if (!es.some((e) => e.isIntersecting)) return
      io.disconnect()
      api.videoThumb(channelId, id).then((r) => (thumb = r.thumb)).catch(() => {})
    })
    io.observe(box)
    return () => io.disconnect()
  })

  async function like() {
    const had = post.liked
    videos.reaction(post.message_id, '👍', !had, true)
    try { await (had ? api.unreact(post.message_id, '👍') : api.react(post.message_id, '👍')) } catch (e) { videos.reaction(post.message_id, '👍', had, true); fail(e) }
  }

  async function toggleList(l: VideoList) {
    const ids = l.items.includes(post.message_id) ? l.items.filter((i) => i !== post.message_id) : [...l.items, post.message_id]
    try { await api.videoListItems(channelId, l.id, ids) } catch (e) { fail(e) }
  }
  const editable = $derived(lists.filter((l) => l.collaborative || l.created_by === app.meId || app.can(serverId, 'manage_messages', channelId)))
  const listMenu = (btn: Element) => menuFrom(btn, editable.length
    ? editable.map((l) => ({ label: l.name, checked: l.items.includes(post.message_id), run: () => void toggleList(l) }))
    : [{ label: 'Aucune liste : créez-en une dans l’onglet Listes', run: () => {} }])
</script>

<article class="vcard" class:seen={!!post.watched_at} class:compact>
  <div class="vthumb" class:music bind:this={box}>
    {#if thumb}<img src={thumb} alt="" loading="lazy" />{:else}<span class="vthumb-ph" aria-hidden="true"><Icon name={music ? 'note' : 'video'} size={28} /></span>{/if}
    {#if playable(post)}<button class="vplay" onclick={() => onwatch(post)} aria-label={verb} title={verb}><span class="vplay-ic" aria-hidden="true"><Icon name="lecture" size={20} /></span></button>{/if}
    <span class="vsrc v-tag">{PROVIDER_LABEL[post.provider] ?? 'Lien'}</span>
    {#if post.watched_at}<span class="vseen v-tag ok"><Icon name="ok" size={12} /> {music ? 'Écouté' : 'Vue'}</span>{/if}
  </div>
  <div class="vbody">
    <a class="vtitle" href={post.url} target="_blank" rel="noopener noreferrer" title={videoTitle(post)}>{videoTitle(post)}</a>
    <div class="v-note truncate">{post.site ? `${post.site} · ` : ''}{post.author.pseudo}</div>
    <div class="vbtns row">
    {#if playable(post)}<button class="plbls-button plbls-button--compact v-rose" onclick={() => onwatch(post)} title="{verb} maintenant"><Icon name={music ? 'note' : 'tele'} size={14} /> {verb}</button>{/if}
    {#if music}
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void solo.play([post]).catch(fail)} title="Écouter seul, sans session"><Icon name="casque" size={14} /> Écouter</button>
      {#if solo.current}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void solo.play([post], { append: true }).catch(fail)}><Icon name="plus" size={14} /> À ma file</button>{/if}
    {/if}
    </div>
    {#if playable(post) && session}<button class="plbls-button plbls-button--compact plbls-button--contour vwatch" onclick={() => void api.watchEnqueue(session, playLink(post), kind).catch(fail)}><Icon name="plus" size={14} /> Ajouter à la file</button>{/if}
    <div class="vacts row">
      <button class="v-ib sm" class:on={post.liked} aria-pressed={post.liked} onclick={() => void like()} aria-label="J’aime" title="J’aime"><span aria-hidden="true">👍</span><span class="v-mono">{post.reactions || ''}</span></button>
      <span class="v-mono v-mute vreplies" title="Réponses dans la discussion"><Icon name="repondre" size={14} /> {post.replies || ''}</span>
      <span class="grow"></span>
      {#if canList}<button class="v-ib sm" onclick={(e) => listMenu(e.currentTarget)} aria-haspopup="menu" aria-label="Ajouter à une liste" title="Ajouter à une liste"><Icon name="file-attente" size={16} /></button>{/if}
      {#if canOrganize}<button class="v-ib sm" onclick={() => void api.videoWatched(channelId, post.message_id, !post.watched_at).catch(fail)} aria-label={post.watched_at ? (music ? 'Marquer comme non écouté' : 'Marquer comme non vue') : (music ? 'Marquer comme écouté par le groupe' : 'Marquer comme vue par le groupe')} title={post.watched_at ? (music ? 'Marquer comme non écouté' : 'Marquer comme non vue') : (music ? 'Marquer comme écouté par le groupe' : 'Marquer comme vue par le groupe')}><Icon name="ok" size={16} /></button>{/if}
      {#if canRemove}<button class="v-ib sm" onclick={(e) => menuFrom(e.currentTarget, [{ label: 'Supprimer', danger: true, run: () => void api.deleteMessage(post.message_id).catch(fail) }])} aria-haspopup="menu" aria-label="Plus d’actions" title="Plus d’actions"><Icon name="plus3" size={16} /></button>{/if}
    </div>
  </div>
</article>
