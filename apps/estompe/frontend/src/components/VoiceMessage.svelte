<script module lang="ts">
  // Un seul message vocal joue à la fois ; à la fin de l'un, le suivant dans le fil démarre seul.
  let current: HTMLAudioElement | null = null
  const LISTENED = 'estompe.voice.listened'
  const SPEEDS = [1, 1.5, 2]
  const LABELS = ['1×', '1,5×', '2×']

  function listened(): string[] {
    try { return JSON.parse(localStorage.getItem(LISTENED) ?? '[]') } catch { return [] }
  }
  function markListened(id: string) {
    try { localStorage.setItem(LISTENED, JSON.stringify([...listened().filter((x) => x !== id), id].slice(-500))) } catch { /* stockage indisponible */ }
  }
  const clock = (sec: number) => `${Math.floor(sec / 60)}:${String(Math.floor(sec % 60)).padStart(2, '0')}`
</script>

<script lang="ts">
  import Icon from './Icon.svelte'
  import { api, usesBearer } from '../lib/api'
  import { seekRatio } from '../lib/files'
  import { notify } from '../lib/ui'
  import { transcriptionOn } from '../blocks/transcription/transcript'
  import type { Attachment } from '../lib/types'

  let { attachment: a, mine = false }: { attachment: Attachment; mine?: boolean } = $props()

  // Transcription (EST-158) : texte du message, replié sur trois lignes ; l'auteur peut le corriger ou le retirer.
  let hidden = $state(false)
  let more = $state(false)
  let busy = $state(false)
  let editing = $state(false)
  let draft = $state('')
  async function transcribe() {
    busy = true
    try { a.transcript = (await api.transcribe(a.id)).transcript; a.transcript_auto = true } catch (e) { notify.error(e, 'La transcription a échoué') } finally { busy = false }
  }
  async function saveDraft() {
    busy = true
    try {
      const r = await api.setTranscript(a.id, draft)
      a.transcript = r.transcript
      a.transcript_auto = false
      editing = false
    } catch (e) { notify.error(e, 'La transcription n\'a pas pu être enregistrée') } finally { busy = false }
  }

  const total = $derived((a.duration_ms ?? 0) / 1000)
  const bars = $derived(a.waveform?.length ? a.waveform : [0])
  const direct = $derived(api.fileUrl(a.id))
  let root = $state<HTMLElement>()
  let audio = $state<HTMLAudioElement>()
  let playing = $state(false)
  let cur = $state(0)
  let speed = $state(0)
  // svelte-ignore state_referenced_locally
  let heard = $state(listened().includes(a.id))
  let blobUrl = $state('')
  let failed = $state(false)

  // En mode Bearer (app Windows), l'audio ne peut pas envoyer l'en-tête : téléchargement puis URL locale.
  async function load() {
    try { blobUrl = await api.fileBlobUrl(a.id) } catch { failed = true }
  }
  $effect(() => {
    if (usesBearer() && !a.expired && !blobUrl) void load()
  })
  const src = $derived(usesBearer() ? blobUrl : direct)
  const progress = $derived(total ? Math.min(1, cur / total) : 0)

  function toggle() {
    if (!audio) return
    if (!audio.paused) return audio.pause()
    if (current && current !== audio) current.pause()
    current = audio
    audio.playbackRate = SPEEDS[speed]
    if (!heard) { markListened(a.id); heard = true }
    void audio.play().catch(() => (failed = true))
  }

  function ended() {
    playing = false
    current = null
    const all = [...document.querySelectorAll<HTMLElement>('.voice-msg')]
    // Le suivant est lancé par son propre bouton : même chemin que pour un clic.
    all[all.indexOf(root!) + 1]?.querySelector<HTMLButtonElement>('[data-voice-play]')?.click()
  }

  function seek(e: MouseEvent) {
    if (!audio || !total) return
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect()
    audio.currentTime = seekRatio(e.clientX, box.left, box.width) * total
    cur = audio.currentTime
  }

  function nextSpeed() {
    speed = (speed + 1) % SPEEDS.length
    if (audio) audio.playbackRate = SPEEDS[speed]
  }
</script>

{#if a.expired}
  <span class="file-chip expired" title="Supprimé du serveur pour faire de la place"><Icon name="mic" size={16} /> Message vocal <span class="small">(fichier expiré)</span></span>
{:else if failed}
  <span class="file-chip expired">
    <Icon name="mic" size={16} /> Message vocal illisible sur cet appareil
    <a class="link small" href={src || direct} download={a.name}>Télécharger</a>
  </span>
{:else}
  <div class="voice-msg" bind:this={root}>
    <button class="ib" data-voice-play onclick={toggle} aria-label={playing ? 'Mettre en pause' : 'Écouter le message vocal'}>
      <Icon name={playing ? 'pause' : 'play'} size={18} />
    </button>
    <button class="wave" onclick={seek} aria-label="Se placer dans le message vocal">
      {#each bars as v, i (i)}<span class:done={i / bars.length < progress} style="height: {Math.max(12, v)}%"></span>{/each}
    </button>
    <span class="time small muted">{clock(playing || cur ? cur : total)}</span>
    <button class="speed small" onclick={nextSpeed} aria-label="Vitesse de lecture">{LABELS[speed]}</button>
    {#if !heard}<span class="unheard" title="Non écouté" aria-label="Non écouté"></span>{/if}
    <a class="ib" href={src || direct} download={a.name} title="Télécharger" aria-label="Télécharger le message vocal"><Icon name="down" size={16} /></a>
    <audio
      bind:this={audio}
      src={src || undefined}
      preload="metadata"
      onplay={() => (playing = true)}
      onpause={() => (playing = false)}
      onended={ended}
      onerror={() => (failed = true)}
      ontimeupdate={() => (cur = audio?.currentTime ?? 0)}
    ></audio>
  </div>
  {#if !transcriptionOn()}{:else if a.transcript}
    <div class="transcript">
      {#if editing}
        <textarea class="plbls-input" rows="3" bind:value={draft} aria-label="Transcription du message vocal"></textarea>
        <button class="small" onclick={saveDraft} disabled={busy}>Enregistrer</button>
        <button class="small" onclick={() => (editing = false)} disabled={busy}>Annuler</button>
      {:else}
        {#if !hidden}<p class="text" class:clamp={!more}>{a.transcript}</p>{/if}
        <span class="small muted">
          {#if a.transcript_auto}Transcription automatique · {/if}
          {#if !hidden && a.transcript.length > 140}<button class="link small" onclick={() => (more = !more)}>{more ? 'Afficher moins' : 'Afficher plus'}</button> · {/if}
          <button class="link small" onclick={() => (hidden = !hidden)}>{hidden ? 'Afficher la transcription' : 'Masquer'}</button>
          {#if mine}· <button class="link small" onclick={() => { draft = a.transcript ?? ''; editing = true }}>Modifier</button>{/if}
        </span>
      {/if}
    </div>
  {:else if busy}
    <span class="small muted">Transcription en cours…</span>
  {:else}
    <button class="link small" onclick={transcribe}>Transcrire</button>
  {/if}
{/if}

<style>
  .voice-msg { display: flex; align-items: center; gap: var(--espace-2); background: var(--surface); border-radius: var(--rayon); padding: 6px 10px; width: min(360px, 100%); }
  .wave { flex: 1; min-width: 80px; height: 32px; display: flex; align-items: center; gap: 2px; border: 0; padding: 0; background: transparent; cursor: pointer; }
  .wave span { flex: 1; min-height: 3px; border-radius: 2px; background: var(--trait); }
  .wave span.done { background: var(--noir); }
  .time { min-width: 3ch; font-variant-numeric: tabular-nums; }
  .speed { border: 0; border-radius: var(--rayon); background: transparent; padding: 2px 6px; cursor: pointer; color: var(--gris); }
  .transcript { width: min(360px, 100%); margin-top: 4px; }
  .text { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .text.clamp { display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .link { border: 0; background: transparent; padding: 0; color: inherit; text-decoration: underline; cursor: pointer; }
  .unheard { width: 8px; height: 8px; border-radius: 50%; background: var(--rose); flex: none; }
</style>
