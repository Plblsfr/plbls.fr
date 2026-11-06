<script lang="ts">
  import Icon from './Icon.svelte'
  import { onDestroy, onMount } from 'svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { Recorder } from '../lib/recorder.svelte'
  import { micCause } from '../blocks/vocal/voice.svelte'
  import { notify } from '../lib/ui'
  import { prefs } from '../lib/prefs.svelte'
  import { transcriptionOn } from '../blocks/transcription/transcript'
  import type { Attachment } from '../lib/types'

  // `hold` : position du doigt à l'appui (téléphone : enregistrer tant qu'on maintient) ; null : clic (PC).
  let { convKey, hold, replyId, onclose }: { convKey: string; hold: { x: number; y: number } | null; replyId?: string; onclose: () => void } = $props()

  const CANCEL_PX = 80
  const LOCK_PX = 60
  const MIN_MS = 1000
  const rec = new Recorder()
  // svelte-ignore state_referenced_locally
  let holding = $state(hold !== null)
  let gesture = $state<'' | 'cancel' | 'lock'>('')
  let sending = $state(false)
  let failed = $state(false)
  let preview = $state('')
  let starting = $state(true)
  // Transcription (EST-158) : le fichier est envoyé dès qu'on la demande, puis réutilisé à l'envoi du message.
  let up: Promise<Attachment> | null = null
  let known = $state<string | null>(null)
  let text = $state('')
  let transcribing = $state(false)
  let tried = false
  // svelte-ignore state_referenced_locally
  const origin = hold

  const bars = $derived(rec.levels.slice(-40))
  const clock = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')}`

  onMount(async () => {
    try {
      await rec.start()
    } catch (e) {
      notify.fail(micCause(e))
      onclose()
      return
    }
    starting = false
  })

  // Le fichier écouté est une URL locale, libérée dès que l'écoute change.
  $effect(() => {
    const t = rec.take
    if (!t) return
    const url = URL.createObjectURL(t.blob)
    preview = url
    return () => URL.revokeObjectURL(url)
  })

  onDestroy(() => rec.cancel())

  function move(e: PointerEvent) {
    if (!holding || origin === null) return
    const dx = e.clientX - origin.x
    const dy = e.clientY - origin.y
    gesture = dx < -CANCEL_PX ? 'cancel' : dy < -LOCK_PX ? 'lock' : ''
  }

  function release() {
    if (!holding) return
    holding = false
    if (gesture === 'cancel') return discard()
    if (gesture === 'lock' || starting) {
      gesture = ''
      return
    }
    if (rec.elapsed < MIN_MS) return discard('Message vocal trop court')
    void finish(true)
  }

  // Le système a repris le geste (appel, notification) : on garde l'enregistrement, en mode verrouillé.
  function interrupted() {
    gesture = 'lock'
    release()
  }

  function discard(msg?: string) {
    rec.cancel()
    if (msg) notify.info(msg)
    onclose()
  }

  /** Arrête l'enregistrement ; `send` enchaîne l'envoi, sinon on passe à l'écoute. */
  async function finish(send: boolean) {
    await rec.stop()
    if (send) await submit()
  }

  // La promesse est mémorisée : la transcription et l'envoi simultanés partagent un seul fichier.
  function ensureUpload(): Promise<Attachment> {
    const t = rec.take!
    return (up ??= api.upload(t.blob, t.name, { duration: t.duration, waveform: t.waveform }).catch((e) => {
      up = null
      throw e
    }))
  }

  async function transcribe(silent = false) {
    if (transcribing || sending || !rec.take) return
    transcribing = true
    try {
      known = text = (await api.transcribe((await ensureUpload()).id)).transcript
    } catch (e) {
      if (!silent) notify.error(e, 'La transcription a échoué')
    } finally {
      transcribing = false
    }
  }

  // Avant l'envoi, la transcription automatique s'affiche dans l'aperçu pour être relue ou retirée.
  $effect(() => {
    if (rec.state === 'review' && transcriptionOn() && prefs.transcribeAuto && !tried) {
      tried = true
      void transcribe(true)
    }
  })

  async function submit() {
    const t = rec.take
    if (!t || sending) return
    sending = true
    failed = false
    try {
      const a = await ensureUpload()
      if (known !== null && text.trim() !== known) await api.setTranscript(a.id, text)
      await app.send(convKey, '', [a.id], replyId)
      // Envoi rapide (sans aperçu) : la transcription arrive ensuite par message.updated.
      if (known === null && !tried && transcriptionOn() && prefs.transcribeAuto) void api.transcribe(a.id).catch(() => {})
      app.reply = null
      onclose()
    } catch (e) {
      // L'enregistrement reste en écoute : « Réessayer » renvoie le même fichier.
      failed = true
      notify.error(e, "Le message vocal n'a pas pu être envoyé")
    } finally {
      sending = false
    }
  }
</script>

<svelte:window onpointermove={move} onpointerup={release} onpointercancel={interrupted} />

<div class="composer-box voice-note" class:cancel={gesture === 'cancel'} class:lock={gesture === 'lock'} role="group" aria-label="Message vocal">
  {#if starting}
    <span class="muted small grow">Ouverture du micro…</span>
  {:else if rec.state === 'recording' && holding}
    <span class="rec"><i></i>{clock(rec.elapsed)}</span>
    <span class="wave grow" aria-hidden="true">{#each bars as v, i (i)}<span style="height: {Math.max(8, v)}%"></span>{/each}</span>
    <span class="hint small">
      {gesture === 'cancel' ? 'Relâcher pour annuler' : gesture === 'lock' ? 'Relâcher pour verrouiller' : '← annuler · ↑ verrouiller'}
    </span>
  {:else if rec.state === 'recording'}
    <button class="ib" onclick={() => discard()} aria-label="Annuler l'enregistrement" title="Annuler"><Icon name="trash" /></button>
    <span class="rec"><i></i>{clock(rec.elapsed)}</span>
    <span class="wave grow" aria-hidden="true">{#each bars as v, i (i)}<span style="height: {Math.max(8, v)}%"></span>{/each}</span>
    <button class="ib" onclick={() => finish(false)} aria-label="Mettre en pause et écouter" title="Pause"><Icon name="pause" /></button>
    <button class="ib solid" onclick={() => finish(true)} aria-label="Envoyer le message vocal" title="Envoyer"><Icon name="up" /></button>
  {:else if rec.state === 'review'}
    <button class="ib" onclick={() => discard()} aria-label="Supprimer le message vocal" title="Supprimer" disabled={sending}><Icon name="trash" /></button>
    <audio class="listen" controls src={preview} preload="metadata"></audio>
    {#if failed}<span class="small" role="alert">Envoi impossible.</span>{/if}
    {#if !transcriptionOn()}{:else if transcribing}<span class="small muted">Transcription…</span>
    {:else if known !== null}<textarea class="plbls-input transcript" rows="2" bind:value={text} placeholder="Transcription (vide : aucune)" aria-label="Transcription du message vocal" disabled={sending}></textarea>
    {:else}<button class="small" onclick={() => transcribe()} disabled={sending}>Transcrire</button>{/if}
    <button class="ib solid" class:is-busy={sending} onclick={submit} disabled={sending} aria-label={failed ? 'Réessayer' : 'Envoyer le message vocal'} title={failed ? 'Réessayer' : 'Envoyer'}><Icon name="up" /></button>
  {/if}
</div>

<style>
  .voice-note { align-items: center; flex-wrap: wrap; }
  .voice-note.cancel { box-shadow: inset 0 0 0 1.5px var(--rose); }
  .voice-note.lock { box-shadow: inset 0 0 0 1.5px var(--noir); }
  .rec { display: inline-flex; align-items: center; gap: 6px; padding: 0 var(--espace-2); font-variant-numeric: tabular-nums; }
  .rec i { width: 8px; height: 8px; border-radius: 50%; background: #d64545; animation: pulse 1.2s ease-in-out infinite; }
  .wave { display: flex; align-items: center; gap: 2px; height: 32px; min-width: 60px; }
  .wave span { flex: 1; min-height: 3px; border-radius: 2px; background: var(--gris); }
  .hint { color: var(--gris); white-space: nowrap; }
  .listen { flex: 1; min-width: 0; height: 36px; }
  .transcript { flex: 1 1 100%; resize: vertical; }
  @keyframes pulse { 50% { opacity: 0.3; } }
</style>
