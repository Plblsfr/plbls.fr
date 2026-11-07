<script lang="ts">
  import Icon from './Icon.svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { renderMarkdown, firstLink } from '../lib/markdown'
  import RoleBadge from './RoleBadge.svelte'
  import BadgeInline from '../blocks/badges/BadgeInline.svelte'
  import { timeOf, isBot } from '../lib/format'
  import { nameColor } from '../lib/roles'
  import { QUICK_REACTIONS, EMOJIS, reactionTitle } from '../blocks/emojis/emoji'
  import { notify } from '../lib/ui'
  import { prefs } from '../lib/prefs.svelte'
  import { emojis } from '../blocks/emojis/emojis.svelte'
  import type { Message, Read } from '../lib/types'
  import Avatar from './Avatar.svelte'
  import { openProfile } from '../lib/profile.svelte'
  import Modal from './Modal.svelte'
  import PollCard from './PollCard.svelte'
  import AttachmentView from './AttachmentView.svelte'
  import EditAttachments, { type EditItem } from './EditAttachments.svelte'
  import VoiceMessage from './VoiceMessage.svelte'
  import LinkCard from '../blocks/apercu-liens/LinkCard.svelte'
  import { placePopup, popupAt } from '../lib/popup'
  import { ctx, menuFrom, selectionIn, selectAll, type MenuItem } from '../lib/ctxmenu.svelte'

  let { message: m, cont, pseudos, convKey, readers = [], fresh = false }: { message: Message; cont: boolean; pseudos: string[]; convKey: string; readers?: Read[]; fresh?: boolean } = $props()

  const MAX_READERS = 5
  let seen = $state(false)
  const readTitle = $derived(readers.map((r) => `${r.pseudo} · ${timeOf(r.read_at)}`).join('\n'))

  const mine = $derived(m.author.id === app.meId && !m.system)
  const mentioned = $derived(!mine && !m.system && !!app.meId && m.mentions?.includes(app.meId))
  const fullDate = $derived(new Date(m.created_at).toLocaleString('fr-FR'))
  const manage = $derived(app.canConv(convKey, 'manage_messages') && !m.system)
  const canPin = $derived(mine || manage)
  const canRemove = $derived(mine || manage)
  const canReact = $derived(app.canConv(convKey, 'add_reactions'))
  const serverId = $derived(convKey.startsWith('c:') ? app.channel(convKey.slice(2))?.server.id : undefined)
  const authorColor = $derived(serverId ? nameColor(app.details[serverId], app.details[serverId]?.members.find((x) => x.id === m.author.id)) : undefined)
  const html = $derived(renderMarkdown(m.content, pseudos, emojis.names()))
  let body = $state<HTMLElement>()

  // Pose l'image des emojis personnalisés (téléchargée avec l'authentification) sur leurs <span class="cemoji">.
  $effect(() => {
    void html
    body?.querySelectorAll<HTMLElement>('.cemoji').forEach((el) => {
      const e = emojis.resolve(el.textContent?.replaceAll(':', '') ?? '', serverId)
      if (!e) return
      emojis.ensure(e.id)
      const src = emojis.src(e.id)
      if (!src) return
      el.style.backgroundImage = `url("${src}")`
      el.setAttribute('role', 'img')
      el.setAttribute('aria-label', `:${e.name}:`)
      el.title = `:${e.name}:`
      el.classList.add('ready')
    })
  })

  function jumpTo(id: string) {
    const el = document.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`)
    if (!el) return notify.info('Ce message est plus haut dans l\'historique.')
    el.scrollIntoView({ block: 'center', behavior: 'smooth' })
    el.classList.add('flash')
    setTimeout(() => el.classList.remove('flash'), 1500)
  }

  async function togglePin() {
    try {
      if (m.pinned_at) await api.unpin(m.id)
      else await api.pin(m.id)
    } catch (e) {
      notify.error(e, 'Épinglage impossible')
    }
  }
  const link = $derived(prefs.showPreviews ? firstLink(m.content) : null)

  let editing = $state(false)
  let draft = $state('')
  let confirmDelete = $state(false)
  let picker = $state(false)
  let pickerFrom: HTMLElement | null = null

  let items = $state<EditItem[]>([])
  let attach = $state<EditAttachments>()
  let proposeDelete = $state(false)
  const uploading = $derived(items.some((i) => !i.att && !i.failed))

  function startEdit() {
    draft = m.content
    items = m.attachments.map((att, key) => ({ key, att, name: att.name }))
    proposeDelete = false
    editing = true
  }

  async function saveEdit() {
    const text = draft.trim()
    if (uploading || items.some((i) => i.failed)) return
    if (!text && items.length === 0) return void (proposeDelete = true)
    const ids = items.map((i) => i.att!.id)
    const same = ids.length === m.attachments.length && ids.every((id, i) => id === m.attachments[i].id)
    if (text === m.content && same) return void (editing = false)
    try {
      const updated = await api.editMessage(m.id, text, same ? undefined : ids)
      Object.assign(m, { content: updated.content, edited_at: updated.edited_at, mentions: updated.mentions, attachments: updated.attachments })
      editing = false
    } catch (e) {
      notify.error(e, 'Modification impossible')
    }
  }

  function editPaste(e: ClipboardEvent) {
    const fs = [...(e.clipboardData?.files ?? [])]
    if (fs.length) { e.preventDefault(); void attach?.add(fs) }
  }

  function editKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void saveEdit() }
    if (e.key === 'Escape') editing = false
  }

  async function remove() {
    try {
      await app.deleteMessage(convKey, m.id)
      confirmDelete = false
    } catch (e) {
      notify.error(e, 'Suppression impossible')
    }
  }

  const copy = (t: string) => void navigator.clipboard.writeText(t).catch((e) => notify.error(e, 'Copie impossible'))
  const menu = (): MenuItem[] => m.system ? [] : [
    { label: 'Répondre', run: () => (app.reply = { key: convKey, message: m }) },
    ...(body && selectionIn(body) ? [{ label: 'Copier la sélection', run: () => copy(selectionIn(body!)) }] : []),
    ...(body ? [{ label: 'Sélectionner le texte', run: () => selectAll(body!) }] : []),
    ...(m.content ? [{ label: 'Copier le message', run: () => copy(m.content) }] : []),
    ...(canPin ? [{ label: m.pinned_at ? 'Désépingler' : 'Épingler', run: togglePin }] : []),
    ...(mine ? [{ label: 'Modifier', run: startEdit }] : []),
    ...(canRemove ? [{ label: 'Supprimer', run: () => (confirmDelete = true), danger: true }] : []),
  ]

  let who = $state<string | null>(null) // emoji dont on affiche les personnes (appui long ou clic droit)
  const whoReaction = $derived(m.reactions.find((r) => r.emoji === who))

  function togglePicker(e: MouseEvent) {
    pickerFrom = e.currentTarget as HTMLElement
    picker = !picker
  }

  function nearPicker(node: HTMLElement) {
    const a = (pickerFrom?.isConnected ? pickerFrom : node.parentElement!).getBoundingClientRect()
    const { left, top } = placePopup(a, node.offsetWidth, node.offsetHeight, window.innerWidth, window.innerHeight)
    node.style.left = `${left}px`
    node.style.top = `${top}px`
    node.querySelector<HTMLElement>('button')?.focus({ preventScroll: true })
  }

  $effect(() => {
    if (!picker) return
    const away = (e: Event) => {
      const t = e.target as Element
      if (!t.closest?.('.msg-picker') && !t.closest?.('[data-picker]')) picker = false
    }
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); picker = false; pickerFrom?.focus() } }
    window.addEventListener('pointerdown', away, true)
    window.addEventListener('scroll', away, true)
    window.addEventListener('keydown', key, true)
    return () => {
      window.removeEventListener('pointerdown', away, true)
      window.removeEventListener('scroll', away, true)
      window.removeEventListener('keydown', key, true)
    }
  })

  function moreActions(e: MouseEvent) {
    const items = menu()
    if (items.length) menuFrom(e.currentTarget as Element, items)
  }

  async function react(emoji: string) {
    picker = false
    try { await app.toggleReaction(m, emoji) } catch (e) { notify.error(e, 'Réaction impossible') }
  }

  // Barre d'actions au-dessus du message ; dessous s'il n'y a pas la place dans le fil (premier message visible).
  let below = $state(false)
  function placeTools(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement
    below = el.getBoundingClientRect().top - (el.closest('.messages')?.getBoundingClientRect().top ?? 0) < 36
  }

  // Spoilers : cliquables et accessibles au clavier ; tabindex est retiré par l'assainissement, on le pose ici.
  function spoilers(node: HTMLElement, _html: string) {
    const mark = () => node.querySelectorAll<HTMLElement>('.spoiler').forEach((s) => (s.tabIndex = 0))
    mark()
    const toggle = (e: Event) => (e.target as HTMLElement).closest('.spoiler')?.classList.toggle('revealed')
    const key = (e: KeyboardEvent) => { if (e.key === 'Enter' || e.key === ' ') toggle(e) }
    node.addEventListener('click', toggle)
    node.addEventListener('keydown', key)
    return { update: mark, destroy() { node.removeEventListener('click', toggle); node.removeEventListener('keydown', key) } }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<article class="msg" class:cont class:head={!cont} class:first={!cont} class:mine class:system={m.system} class:ment={mentioned} class:v-anim-monte={fresh} class:editing data-id={m.id} tabindex="-1" use:ctx={menu} onmouseenter={placeTools}>
  <div class="msg-gutter">
    {#if !cont && !m.system}<button class="profile-link msg-av" onclick={() => openProfile(m.author.id, serverId)} aria-label="Profil de {m.author.pseudo}"><Avatar pseudo={m.author.pseudo} userId={m.author.id} {serverId} /></button>{/if}
    {#if m.pinned_at && (cont || m.system)}<span class="pinned" role="img" aria-label="Message épinglé" title="Message épinglé"><Icon name="epingle" size={13} /></span>{/if}
    <time class="msg-t" datetime={m.created_at} title={fullDate}>{timeOf(m.created_at)}</time>
  </div>
  <div class="msg-col">
    {#if m.reply_to}
      <button class="reply-ref" onclick={() => jumpTo(m.reply_to!.id)} title="Aller au message">
        <Icon name="repondre" size={14} /><span class="reply-author">{m.reply_to.author.pseudo}</span><span class="reply-text">{m.reply_to.content}</span>
      </button>
    {/if}
    {#if !cont && !m.system}
      <div class="msg-head">
        <button class="profile-link msg-author" style:color={authorColor} onclick={() => openProfile(m.author.id, serverId)}>{m.author.pseudo}</button>
        {#if isBot(m.author.id)}<RoleBadge kind="bot" />{/if}
        <BadgeInline userId={m.author.id} />
        {#if m.pinned_at}<span class="pinned" role="img" aria-label="Message épinglé" title="Message épinglé"><Icon name="epingle" size={13} /></span>{/if}
        <time class="msg-time" datetime={m.created_at} title={fullDate}>{timeOf(m.created_at)}</time>
      </div>
    {/if}

    {#if editing}
      <div class="edit-box">
        <EditAttachments bind:this={attach} bind:items canAttach={app.canConv(convKey, 'attach_files')} />
        <textarea class="plbls-textarea" rows="2" bind:value={draft} onkeydown={editKey} onpaste={editPaste} maxlength="4000" aria-label="Modifier le message"></textarea>
        {#if proposeDelete}
          <p class="edit-empty">Un message ne peut pas être vide. <button class="v-btn sm sec" onclick={() => { editing = false; proposeDelete = false; confirmDelete = true }}>Supprimer le message</button></p>
        {/if}
        <div class="edit-actions">
          {#if app.canConv(convKey, 'attach_files')}<label class="v-ib sm" title="Joindre un fichier"><Icon name="trombone" size={16} /><span class="plbls-sr-only">Joindre un fichier</span><input type="file" multiple hidden onchange={(e) => { void attach?.add([...(e.currentTarget.files ?? [])]); e.currentTarget.value = '' }} /></label>{/if}
          <button class="v-btn sm" disabled={uploading} onclick={saveEdit}>Enregistrer</button>
          <button class="v-btn sm sec" onclick={() => (editing = false)}>Annuler</button>
          <span class="edit-hint"><kbd class="v-kbd">Entrée</kbd> pour valider · <kbd class="v-kbd">Échap</kbd> pour annuler</span>
        </div>
      </div>
    {:else if m.poll}
      <PollCard message={m} {convKey} {pseudos} />
    {:else}
      {#if m.content}
        <div class="msg-body" bind:this={body} use:spoilers={html}>
          {@html html}
          {#if m.edited_at}<span class="edited" title="Modifié le {new Date(m.edited_at).toLocaleString('fr-FR')}"> (modifié)</span>{/if}
        </div>
      {/if}
      {#if m.attachments.length}
        <div class="attachments">
          {#each m.attachments as a (a.id)}{#if a.kind === 'voice'}<VoiceMessage attachment={a} {mine} />{:else}<AttachmentView attachment={a} />{/if}{/each}
        </div>
      {/if}
      {#if link}<LinkCard url={link} />{/if}
    {/if}

    {#if m.reactions.length || readers.length}
      <div class="msg-foot">
        {#if m.reactions.length}
          <div class="reactions">
            {#each m.reactions as r (r.emoji)}
              <button class="react" class:me={r.me} onclick={() => react(r.emoji)} oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); who = r.emoji }} title={reactionTitle(r, app.meId)} aria-pressed={r.me} aria-label="{r.emoji} {r.count}">{r.emoji} {r.count}</button>
            {/each}
            {#if canReact}<button class="react-add" data-picker onclick={togglePicker} aria-label="Ajouter une réaction" aria-expanded={picker} title="Ajouter une réaction"><Icon name="sourire" size={15} /></button>{/if}
            {#if whoReaction}
              <div class="v-menu msg-pop" use:popupAt>
                <div class="v-seg msg-pop-tabs">
                  {#each m.reactions as r (r.emoji)}<button class:on={r.emoji === who} aria-pressed={r.emoji === who} onclick={() => (who = r.emoji)}>{r.emoji} {r.count}</button>{/each}
                </div>
                {#each whoReaction.users as u (u.id)}<div class="v-mi msg-pop-who"><Avatar pseudo={u.pseudo} userId={u.id} {serverId} size="sm" />{u.id === app.meId ? 'Toi' : u.pseudo}</div>{/each}
                <div class="v-sep"></div>
                <button class="v-mi" onclick={() => (who = null)}>Fermer</button>
              </div>
            {/if}
          </div>
        {/if}
        {#if readers.length}
          <div class="readers">
            <button class="readers-btn" onclick={() => (seen = !seen)} title={readTitle} aria-label="Vu par {readers.map((r) => r.pseudo).join(', ')}" aria-expanded={seen}>
              {#each readers.slice(0, MAX_READERS) as r (r.user_id)}<Avatar pseudo={r.pseudo} userId={r.user_id} {serverId} />{/each}
              {#if readers.length > MAX_READERS}<span class="readers-more">+{readers.length - MAX_READERS}</span>{/if}
            </button>
            {#if seen}
              <div class="v-menu msg-pop" use:popupAt>
                <div class="v-mh">Vu par</div>
                {#each readers as r (r.user_id)}<div class="v-mi msg-pop-who"><Avatar pseudo={r.pseudo} userId={r.user_id} {serverId} size="sm" /><span class="grow">{r.pseudo}</span><span class="v-mono v-mute2">{timeOf(r.read_at)}</span></div>{/each}
                <div class="v-sep"></div>
                <button class="v-mi" onclick={() => (seen = false)}>Fermer</button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if !editing}
    <div class="msg-tools" class:below role="toolbar" aria-label="Actions sur le message">
      {#if canReact}
        {#each QUICK_REACTIONS.slice(0, 3) as e}<button class="v-ib sm msg-tool" onclick={() => react(e)} aria-label="Réagir avec {e}" title="Réagir avec {e}">{e}</button>{/each}
        <span class="msg-tools-sep" aria-hidden="true"></span>
      {/if}
      <button class="v-ib sm msg-tool" onclick={() => (app.reply = { key: convKey, message: m })} aria-label="Répondre" title="Répondre"><Icon name="repondre" size={16} /></button>
      {#if canPin}<button class="v-ib sm msg-tool" onclick={togglePin} aria-label={m.pinned_at ? 'Désépingler' : 'Épingler'} aria-pressed={!!m.pinned_at} title={m.pinned_at ? 'Désépingler' : 'Épingler'}><Icon name="epingle" size={16} /></button>{/if}
      {#if canReact}<button class="v-ib sm msg-tool" data-picker onclick={togglePicker} aria-label="Plus de réactions" aria-expanded={picker} title="Plus de réactions"><Icon name="sourire" size={16} /></button>{/if}
      {#if mine}<button class="v-ib sm msg-tool" onclick={startEdit} aria-label="Modifier" title="Modifier"><Icon name="crayon" size={16} /></button>{/if}
      {#if canRemove}<button class="v-ib sm msg-tool msg-tool-dgr" onclick={() => (confirmDelete = true)} aria-label="Supprimer" title="Supprimer"><Icon name="corbeille" size={16} /></button>{/if}
      {#if !m.system}<button class="v-ib sm msg-tool" onclick={moreActions} aria-label="Plus d'actions" aria-haspopup="menu" title="Plus d'actions"><Icon name="plus3" size={16} /></button>{/if}
    </div>
  {/if}
  {#if picker}
    <div class="v-menu msg-picker" use:nearPicker role="dialog" aria-label="Choisir une réaction">
      <div class="msg-picker-grid">
        {#each EMOJIS as e}<button onclick={() => react(e)} aria-label={e}>{e}</button>{/each}
      </div>
    </div>
  {/if}
</article>

{#if confirmDelete}
  <Modal title="Supprimer ce message ?" onclose={() => (confirmDelete = false)}>
    <p class="msg-confirm-text">Cette action est définitive.</p>
    <div class="msg-confirm-actions">
      <button class="plbls-button plbls-button--contour" onclick={() => (confirmDelete = false)}>Annuler</button>
      <button class="plbls-button plbls-button--danger" onclick={remove}>Supprimer</button>
    </div>
  </Modal>
{/if}
