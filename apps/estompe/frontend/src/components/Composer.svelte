<script lang="ts">
  import Loader from './Loader.svelte'
  import PollModal from './PollModal.svelte'
  import VoiceNote from './VoiceNote.svelte'
  import Icon from './Icon.svelte'
  import { tick, onMount, onDestroy } from 'svelte'
  import { app } from '../lib/store.svelte'
  import { api, ApiError } from '../lib/api'
    import { formatSize } from '../lib/format'
  import { groupesCites, insertMention, mentionAt, SEUIL_CONFIRMATION, suggest } from '../lib/mention'
  import { confirmer } from '../lib/confirm.svelte'
  import { codeAt, completeCode, emojiByCode, loadEmojiRows, searchEmojis, type EmojiRow } from '../blocks/emojis/emoji'
  import { emojis } from '../blocks/emojis/emojis.svelte'
  import GifPicker from '../blocks/gifs/GifPicker.svelte'
  import EmojiPicker from '../blocks/emojis/EmojiPicker.svelte'
  import type { GifResult } from '../lib/types'
  import { uploadAttachment } from '../blocks/videos/video'
  import { notify } from '../lib/ui'
  import { clearDraft, keepSession, markDraft, persistFiles, restoreFiles, sessionFiles, type DraftFile } from '../lib/drafts.svelte'

  let { convKey, placeholder }: { convKey: string; placeholder: string } = $props()

  type Pending = DraftFile

  // La vue est recréée à chaque changement de conversation ({#key} dans ChatView).
  // svelte-ignore state_referenced_locally
  const DRAFT = `estompe.draft.${convKey}`
  const loadDraft = () => { try { return localStorage.getItem(DRAFT) ?? '' } catch { return '' } }
  const saveDraft = (v: string) => { try { v ? localStorage.setItem(DRAFT, v) : localStorage.removeItem(DRAFT) } catch { /* stockage indisponible */ } }

  // svelte-ignore state_referenced_locally
  let text = $state(loadDraft())
  let gif = $state(false)
  let lastTyping = 0
  const reply = $derived(app.reply?.key === convKey ? app.reply.message : null)
  // Le champ est retiré pendant l'enregistrement vocal : sa référence est mise à jour par bind:this.
  // svelte-ignore non_reactive_update
  let ta: HTMLTextAreaElement
  // svelte-ignore state_referenced_locally
  let files = $state<Pending[]>(sessionFiles(convKey) ?? [])
  // svelte-ignore state_referenced_locally
  keepSession(convKey, files)
  let warned = false
  let ready = $state(false) // pas d'enregistrement avant la lecture du brouillon de fichiers
  let sending = $state(false)
  let dragging = $state(false)
  let emoji = $state(false)
  let caret = $state(0)
  let sel = $state(0)
  // svelte-ignore state_referenced_locally
  let seq = Math.max(0, ...files.map((f) => f.key))
  let pollOpen = $state(false)
  let voiceOn = $state(false)
  let voiceHold = $state<{ x: number; y: number } | null>(null)

  const mention = $derived(mentionAt(text, caret))
  const options = $derived(mention ? suggest(mention.query, [...app.pseudosOf(convKey), ...app.groupSuggestionsOf(convKey)], app.pseudo) : [])
  const GROUPES: Record<string, string> = { everyone: 'tout le salon', here: 'membres en ligne' }
  const canAttach = $derived(app.canConv(convKey, 'attach_files'))
  const canPoll = $derived(app.canConv(convKey, 'create_polls'))
  let rows = $state<EmojiRow[]>([])
  const code = $derived(codeAt(text, caret))
  const codeOptions = $derived(code && rows.length ? searchEmojis(rows, code.query, 8) : [])
  const uploading = $derived(files.some((f) => !f.id && !f.failed))
  const canSend = $derived(!sending && !uploading && !files.some((f) => f.failed) && (text.trim().length > 0 || files.length > 0))

  $effect(() => { void options; void codeOptions; sel = 0 })

  // Un smiley personnalisé du même nom garde la priorité sur le smiley standard.
  const known = (name: string) => (emojis.resolve(name) ? undefined : emojiByCode(name))

  /** Convertit « :nom: » dès que le second « : » est tapé. */
  function convert() {
    const r = completeCode(ta.value, ta.selectionStart, known)
    if (!r) return
    text = r.text
    void tick().then(() => { ta.setSelectionRange(r.caret, r.caret); sync() })
  }

  function resize() {
    ta.style.height = 'auto'
    ta.style.height = Math.min(ta.scrollHeight, 220) + 'px'
  }

  function sync() {
    caret = ta.selectionStart
    resize()
  }

  // Brouillon conservé par conversation, et signal « écrit… » au plus toutes les 3 s.
  $effect(() => {
    const v = text
    const t = setTimeout(() => saveDraft(v), 300)
    return () => clearTimeout(t)
  })
  function typed() {
    if (!text.trim() || Date.now() - lastTyping < 3000) return
    lastTyping = Date.now()
    api.typing(convKey).catch(() => {})
  }
  onMount(() => {
    resize()
    void loadEmojiRows().then((r) => { rows = r })
    // Redémarrage : les fichiers du brouillon sont renvoyés (jamais deux fois : la session les garde en mémoire).
    if (files.length) ready = true
    else void restoreFiles(convKey).then((fs) => { if (!files.length && fs.length) void addFiles(fs); ready = true })
  })
  const release = (f: Pending) => f.preview && URL.revokeObjectURL(f.preview)
  const drop = (f: Pending) => { release(f); files = files.filter((x) => x.key !== f.key); keepSession(convKey, files) }
  // Les aperçus restent valides tant que le brouillon existe : ils ne sont libérés qu'à l'envoi ou à la suppression.
  onDestroy(() => saveDraft(text)) // changement de conversation avant la fin du délai d'enregistrement

  $effect(() => {
    const kept = files.filter((f) => f.file && !f.failed).map((f) => f.file!)
    markDraft(convKey, files.length > 0 || text.length > 0)
    if (ready) void persistFiles(convKey, kept).then((ok) => {
      if (!ok && !warned) { warned = true; notify.info('Fichiers trop lourds : gardés pendant la session, perdus au redémarrage') }
    })
  })

  function closeGif() {
    gif = false
    ta?.focus()
  }

  async function sendGif(g: GifResult) {
    closeGif()
    try {
      const a = await api.gifImport(g.url)
      await app.send(convKey, '', [a.id], reply?.id)
      app.reply = null
    } catch (e) {
      notify.error(e, "Le GIF n'a pas pu être envoyé")
    }
  }

  async function addFiles(list: Iterable<File>) {
    if (!canAttach) return
    for (const file of list) {
      const media = /^(image\/(png|jpe?g|gif|webp)|video\/(mp4|webm))$/.test(file.type)
      files.push({ key: ++seq, file, name: file.name, size: file.size, preview: media ? URL.createObjectURL(file) : undefined, video: file.type.startsWith('video/') })
      const entry = files[files.length - 1] // proxy réactif de l'élément ajouté
      try {
        const a = await uploadAttachment(file)
        entry.id = a.id
        entry.name = a.name
        entry.size = a.size
      } catch (e) {
        entry.failed = true
        notify.error(e instanceof ApiError ? e : new Error(), `Envoi de « ${file.name} » impossible`)
        if (!(e instanceof ApiError) && e instanceof Error && e.message) notify.info(e.message)
      }
    }
  }

  function onpaste(e: ClipboardEvent) {
    const fs = [...(e.clipboardData?.files ?? [])]
    if (fs.length) { e.preventDefault(); void addFiles(fs) }
  }

  function ondrop(e: DragEvent) {
    e.preventDefault()
    dragging = false
    if (e.dataTransfer?.files.length) void addFiles(e.dataTransfer.files)
  }

  function pick(pseudo: string) {
    if (!mention) return
    const r = insertMention(text, caret, mention.start, pseudo)
    text = r.text
    void tick().then(() => { ta.focus(); ta.setSelectionRange(r.caret, r.caret); sync() })
  }

  function pickCode(r: EmojiRow) {
    if (!code) return
    const at = code.start + r[0].length
    text = text.slice(0, code.start) + r[0] + text.slice(caret)
    void tick().then(() => { ta.focus(); ta.setSelectionRange(at, at); sync() })
  }

  async function send() {
    if (!canSend) return
    if (app.canConv(convKey, 'mention_everyone')) {
      const touches = Math.max(0, ...groupesCites(text).map((g) => app.groupCountOf(convKey, g) ?? 0))
      if (touches > SEUIL_CONFIRMATION && !(await confirmer({ titre: 'Mentionner beaucoup de monde', message: `Ce message notifie ${touches} personnes. L'envoyer ?`, action: 'Envoyer' }))) return
    }
    sending = true
    try {
      await app.send(convKey, text.trim(), files.map((f) => f.id!), reply?.id)
      text = ''
      saveDraft('')
      app.reply = null
      emoji = false
      files.forEach(release)
      files = []
      clearDraft(convKey)
      keepSession(convKey, files)
      await tick()
      resize()
    } catch (e) {
      notify.error(e, "Le message n'a pas pu être envoyé")
    } finally {
      sending = false
      ta.focus()
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (options.length) {
      if (e.key === 'ArrowDown') { e.preventDefault(); sel = (sel + 1) % options.length; return }
      if (e.key === 'ArrowUp') { e.preventDefault(); sel = (sel - 1 + options.length) % options.length; return }
      if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); pick(options[sel]); return }
      if (e.key === 'Escape') { e.preventDefault(); caret = 0; return }
    }
    if (codeOptions.length) {
      if (e.key === 'ArrowDown') { e.preventDefault(); sel = (sel + 1) % codeOptions.length; return }
      if (e.key === 'ArrowUp') { e.preventDefault(); sel = (sel - 1 + codeOptions.length) % codeOptions.length; return }
      if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); pickCode(codeOptions[sel]); return }
      if (e.key === 'Escape') { e.preventDefault(); caret = 0; return }
    }
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); void send() }
  }

  /** Insère un smiley (ou un code de smiley personnalisé) au curseur ; `keep` (Maj+clic) laisse le sélecteur ouvert. */
  function addEmoji(e: string, keep = false) {
    const at = ta.selectionStart
    text = text.slice(0, at) + e + text.slice(ta.selectionEnd)
    if (!keep) emoji = false
    void tick().then(() => { if (!keep) ta.focus(); ta.setSelectionRange(at + e.length, at + e.length); sync() })
  }

  function closeEmoji() {
    emoji = false
    ta?.focus()
  }

  /** Téléphone : appui maintenu pour enregistrer. PC : un clic démarre (voir onclick). */
  function voicePress(e: PointerEvent) {
    if (e.pointerType === 'mouse') return
    voiceHold = { x: e.clientX, y: e.clientY }
    voiceOn = true
  }

  function voiceClick() {
    if (voiceOn) return
    voiceHold = null
    voiceOn = true
  }

  async function closeVoice() {
    voiceOn = false
    await tick()
    ta?.focus()
  }
</script>

<svelte:window onpointerdown={(e) => {
  const t = e.target as Element
  if (gif && !t.closest?.('.gif-anchor')) closeGif()
  if (emoji && !t.closest?.('.emoji-anchor')) emoji = false
}} />
<div
  class="composer"
  class:dropping={dragging}
  role="group"
  aria-label="Nouveau message"
  ondragover={(e) => { e.preventDefault(); dragging = true }}
  ondragleave={() => (dragging = false)}
  {ondrop}
>
  {#if options.length}
    <div class="popup" role="listbox" aria-label="Mentionner">
      {#each options as o, i}
        <button role="option" aria-selected={i === sel} class:sel={i === sel} onmousedown={(e) => { e.preventDefault(); pick(o) }}>@{o}{#if GROUPES[o]} <span class="muted">{GROUPES[o]}{#if app.groupCountOf(convKey, o) !== null} — {app.groupCountOf(convKey, o)}{o === 'here' ? ' en ligne' : ' personnes'}{/if}</span>{/if}</button>
      {/each}
    </div>
  {/if}
  {#if codeOptions.length}
    <div class="popup" role="listbox" aria-label="Smileys">
      {#each codeOptions as r, i}
        <button role="option" aria-selected={i === sel} class:sel={i === sel} onmousedown={(e) => { e.preventDefault(); pickCode(r) }}>{r[0]} <span class="muted">:{r[1]}:</span></button>
      {/each}
    </div>
  {/if}
  {#if pollOpen}<PollModal {convKey} onclose={() => (pollOpen = false)} />{/if}
    {#if reply}
    <div class="reply-bar">
      <Icon name="reply" size={14} /><span class="truncate grow">Réponse à <strong>{reply.author.pseudo}</strong> : {reply.content.slice(0, 80)}</span>
      <button class="ib sm" onclick={() => (app.reply = null)} aria-label="Annuler la réponse"><Icon name="x" size={14} /></button>
    </div>
  {/if}

  {#if files.length}
    <div class="pending">
      {#each files as f (f.key)}
        {#if f.preview}
          <span class="thumb" class:failed={f.failed} title="{f.name} · {formatSize(f.size)}">
            {#if f.video}<video src={f.preview + '#t=0.1'} muted preload="metadata"></video>{:else}<img src={f.preview} alt={f.name} />{/if}
            {#if f.video}<span class="thumb-badge"><Icon name="film" size={12} /></span>{/if}
            {#if !f.id && !f.failed}<span class="thumb-wait"><Loader size="sm" label="Envoi de {f.name}" /></span>{:else if f.failed}<span class="thumb-wait small">échec</span>{/if}
            <button onclick={() => drop(f)} aria-label="Retirer {f.name}"><Icon name="x" size={14} /></button>
          </span>
        {:else}
        <span class="file-chip" class:expired={f.failed}>
          <Icon name="file" size={16} /> <span class="truncate">{f.name}</span> <span class="muted small">{f.failed ? 'échec' : f.id ? formatSize(f.size) : 'envoi…'}</span>
          <button onclick={() => drop(f)} aria-label="Retirer {f.name}"><Icon name="x" size={14} /></button>
        </span>
        {/if}
      {/each}
    </div>
  {/if}

  {#if voiceOn}
    <VoiceNote {convKey} hold={voiceHold} replyId={reply?.id} onclose={closeVoice} />
  {:else}
  <div class="composer-box">
    {#if canAttach}
    <label class="ib" title="Joindre un fichier (10 Mo max)">
      <Icon name="clip" /><span class="plbls-sr-only">Joindre un fichier</span>
      <input type="file" multiple hidden onchange={(e) => { void addFiles([...(e.currentTarget.files ?? [])]); e.currentTarget.value = '' }} />
    </label>
    {/if}
    <textarea
      bind:this={ta}
      bind:value={text}
      rows="1"
      maxlength="4000"
      {placeholder}
      aria-label={placeholder}
      oninput={() => { sync(); typed(); convert() }}
      onclick={sync}
      onkeyup={(e) => { if (!['Enter', 'ArrowUp', 'ArrowDown', 'Tab'].includes(e.key)) sync() }}
      {onkeydown}
      {onpaste}
    ></textarea>
    {#if canPoll}<button class="ib" onclick={() => (pollOpen = true)} aria-label="Sondage" aria-haspopup="dialog" title="Créer un sondage"><Icon name="chart" /></button>{/if}
    <span class="gif-anchor">
      {#if app.on('gifs')}<button class="ib txt" onclick={() => { gif ? closeGif() : (gif = true); emoji = false }} aria-label="GIF" aria-expanded={gif}>GIF</button>{/if}
      {#if gif}<GifPicker onpick={sendGif} onclose={closeGif} />{/if}
    </span>
    <span class="emoji-anchor">
      <button class="ib" onclick={() => { emoji ? closeEmoji() : (emoji = true); gif = false }} aria-label="Emojis" aria-expanded={emoji} aria-haspopup="dialog"><Icon name="smile" /></button>
      {#if emoji}<EmojiPicker onpick={addEmoji} onclose={closeEmoji} />{/if}
    </span>
    {#if text.trim() || files.length}
      <button class="ib solid" class:is-busy={sending} disabled={!canSend} onclick={send} aria-label="Envoyer" title="Envoyer"><Icon name="up" /></button>
    {:else if canAttach}
      <button class="ib" onpointerdown={voicePress} onclick={voiceClick} aria-label="Message vocal" title="Message vocal"><Icon name="mic" /></button>
    {/if}
  </div>
  {/if}
  {#if text.length > 3500}<div class="muted small" style="text-align:right">{text.length} / 4000</div>{/if}
</div>
