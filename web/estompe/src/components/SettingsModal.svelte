<script lang="ts">
  import { app } from '../lib/store.svelte'
  import CameraPreview from './CameraPreview.svelte'
  import CameraFxControls from './CameraFxControls.svelte'
  import CameraOverlayControls from './CameraOverlayControls.svelte'
  import { prefs, type Density, type Theme, type NoiseMode, type ScreenRes, type CameraQuality, type VideoCodecPref, type SoundChoice } from '../lib/prefs.svelte'
  import { playTone, SOUND_PRESETS } from '../blocks/notifications/notify'
  import { avatars, squareAvatar } from '../blocks/avatars/avatars.svelte'
  import { api } from '../lib/api'
  import Avatar from './Avatar.svelte'
  import { voice } from '../blocks/vocal/voice.svelte'
  import { micTest, MIC_TEST_MAX_S } from '../lib/mictest.svelte'
  import { onDestroy } from 'svelte'
  import { logout } from '../lib/auth'
  import { notify } from '../lib/ui'
  import { platform } from '../lib/platform'
  import { settingsNav, SETTINGS_TABS, type SettingsTab } from '../lib/settings.svelte'
  import Modal from './Modal.svelte'
  import SettingsNotifications from '../blocks/notifications/SettingsNotifications.svelte'
  import SettingsDebug from './SettingsDebug.svelte'
  import SettingsReports from '../blocks/signalements/SettingsReports.svelte'

  const { settingsUpdate: Update, settingsPtt: Ptt, settingsShare: Share, settingsVoice: Player } = platform.slots
  import { transcriptionOn } from '../blocks/transcription/transcript'

  let { onclose }: { onclose: () => void } = $props()

  // Onglet demandé (lien depuis un message), sinon le dernier ouvert. Sur mobile, on commence par la liste des catégories.
  const asked = settingsNav.request
  settingsNav.request = ''
  const tabs = SETTINGS_TABS
  const valid = (t: string): t is SettingsTab => SETTINGS_TABS.some((x) => x.id === t)
  let tab = $state<SettingsTab>(valid(asked) ? asked : valid(prefs.settingsTab) ? prefs.settingsTab : 'account')
  let view = $state<'list' | 'page'>(asked || !(typeof matchMedia !== 'undefined' && matchMedia('(max-width: 700px)').matches) ? 'page' : 'list')
  function go(t: SettingsTab) {
    tab = t
    view = 'page'
    prefs.settingsTab = t
    prefs.save()
  }
  // Un lien « ouvrir tel onglet » reçu pendant que la fenêtre est déjà ouverte.
  $effect(() => {
    const t = settingsNav.request
    if (!t) return
    settingsNav.request = ''
    go(t)
  })

  let avatarBusy = $state(false)
  async function pickAvatar(e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    avatarBusy = true
    try {
      await api.setAvatar(await squareAvatar(file))
    } catch {
      notify.info('Image impossible à utiliser comme avatar.')
    } finally {
      avatarBusy = false
    }
  }
  let serverAvatarBusy = $state('')
  async function pickServerAvatar(sid: string, e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    serverAvatarBusy = sid
    try {
      await api.setServerAvatar(sid, await squareAvatar(file))
    } catch {
      notify.info('Image impossible à utiliser comme photo de serveur.')
    } finally {
      serverAvatarBusy = ''
    }
  }
  async function removeServerAvatar(sid: string) {
    serverAvatarBusy = sid
    try { await api.deleteServerAvatar(sid) } finally { serverAvatarBusy = '' }
  }
  async function removeAvatar() {
    avatarBusy = true
    try { await api.deleteAvatar() } finally { avatarBusy = false }
  }

  let mics = $state<MediaDeviceInfo[]>([])
  let speakers = $state<MediaDeviceInfo[]>([])
  let cams = $state<MediaDeviceInfo[]>([])
  const canSink = typeof HTMLMediaElement !== 'undefined' && 'setSinkId' in HTMLMediaElement.prototype
  let capturing = $state(false)

  async function listDevices() {
    try {
      const all = await navigator.mediaDevices.enumerateDevices()
      mics = all.filter((d) => d.kind === 'audioinput')
      speakers = all.filter((d) => d.kind === 'audiooutput')
      cams = all.filter((d) => d.kind === 'videoinput')
    } catch { /* périphériques indisponibles */ }
  }
  void listDevices()

  const label = (d: MediaDeviceInfo, i: number) => d.label || `Périphérique ${i + 1}`
  const pick = (id: string) => [...mics, ...speakers, ...cams].find((d) => d.deviceId === id)?.label ?? ''
  function change(kind: 'audioinput' | 'audiooutput' | 'videoinput') {
    prefs.save()
    void voice.switchDevice(kind)
    micTest.refresh()
  }
  function noiseChanged() {
    prefs.save()
    void voice.reconfigureMic()
    micTest.refresh()
  }
  function pttChanged() {
    prefs.save()
    void voice.refreshPtt()
    micTest.refresh()
  }
  // le test s'arrête en quittant les paramètres ou en rejoignant un vocal
  onDestroy(() => micTest.reset())
  $effect(() => { if (voice.state !== 'idle') micTest.stop() })
  const dbPct = (db: number) => Math.max(0, Math.min(100, ((db + 80) / 80) * 100))
  // Activation du micro : toujours ouvert, détection de voix ou push-to-talk.
  const micMode = $derived(prefs.ptt ? 'ptt' : prefs.vad ? 'vad' : 'always')
  function setMicMode(m: string) {
    prefs.ptt = m === 'ptt'
    prefs.vad = m === 'vad'
    pttChanged()
  }
  const restartCamera = () => { prefs.save(); void voice.restartCamera() }

  /** Son personnalisé : fichier audio de 150 Ko au plus, gardé sur cet appareil. */
  function pickSound(kind: 'join' | 'leave') {
    return (e: Event) => {
      const input = e.currentTarget as HTMLInputElement
      const f = input.files?.[0]
      input.value = ''
      if (!f) return
      if (!f.type.startsWith('audio/') || f.size > 150 << 10) return notify.info('Choisis un fichier audio de 150 Ko au plus.')
      const r = new FileReader()
      r.onload = () => {
        if (kind === 'join') { prefs.customJoin = r.result as string; prefs.joinSound = 'custom' }
        else { prefs.customLeave = r.result as string; prefs.leaveSound = 'custom' }
        prefs.save()
        playTone(kind, 'custom')
      }
      r.readAsDataURL(f)
    }
  }
  async function captureKey(e: KeyboardEvent) {
    e.preventDefault()
    capturing = false
    const err = platform.globalPtt ? await platform.globalPtt.check(e.code) : ''
    if (err) return notify.info(err)
    prefs.pttKey = e.code
    pttChanged()
  }
</script>

<Modal title="Paramètres" wide {onclose}>
  <div class="set" data-view={view}>
    <nav class="set-nav" aria-label="Catégories de réglages">
      {#each tabs as t (t.id)}
        <button class="set-tab" class:active={tab === t.id} aria-current={tab === t.id ? 'page' : undefined} onclick={() => go(t.id)}>{t.label}</button>
      {/each}
    </nav>
    <section class="set-page stack" aria-labelledby="set-h">
      <button class="plbls-button plbls-button--compact plbls-button--discret set-back" onclick={() => (view = 'list')}>‹ Paramètres</button>
      <h2 class="set-h" id="set-h">{SETTINGS_TABS.find((t) => t.id === tab)?.label}</h2>

      {#if tab === 'account'}
        <div class="row">
          <Avatar pseudo={app.pseudo} userId={app.meId} size="lg" />
          <div class="stack">
            <span>Connecté en tant que <strong>{app.pseudo}</strong></span>
            <div class="row">
              <label class="plbls-button plbls-button--compact plbls-button--contour" class:is-busy={avatarBusy}>
                Changer l'avatar<input type="file" accept="image/*" hidden onchange={pickAvatar} disabled={avatarBusy} />
              </label>
              <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={removeAvatar} disabled={avatarBusy}>Supprimer</button>
            </div>
          </div>
        </div>
        {#if app.servers.length}
          <strong>Photo de profil par serveur</strong>
          <p class="small muted">Sans photo propre à un serveur, ta photo globale y est utilisée.</p>
          {#each app.servers as sv (sv.id)}
            <div class="row">
              <Avatar pseudo={app.pseudo} userId={app.meId} serverId={sv.id} size="sm" />
              <span class="truncate">{sv.name}</span>
              <label class="plbls-button plbls-button--compact plbls-button--contour" class:is-busy={serverAvatarBusy === sv.id}>
                Changer<input type="file" accept="image/*" hidden onchange={(e) => pickServerAvatar(sv.id, e)} disabled={serverAvatarBusy === sv.id} />
              </label>
              {#if avatars.serverVersion(app.meId, sv.id)}
                <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => removeServerAvatar(sv.id)} disabled={serverAvatarBusy === sv.id}>Retirer</button>
              {/if}
            </div>
          {/each}
        {/if}
        <button class="plbls-button plbls-button--contour" onclick={() => logout()}>Se déconnecter</button>

      {:else if tab === 'notifications'}
        <SettingsNotifications />

      {:else if tab === 'debug'}
        <SettingsDebug />

      {:else if tab === 'reports'}
        {#if app.on('signalements')}<SettingsReports />{/if}

      {:else if tab === 'appearance'}
        <label class="plbls-toggle">
          <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.showPreviews} onchange={() => prefs.save()} />
          <span class="plbls-toggle__piste"></span>
          <span class="plbls-toggle__texte">Aperçu des liens</span>
        </label>
        {#if transcriptionOn()}<label class="plbls-toggle">
          <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.transcribeAuto} onchange={() => prefs.save()} />
          <span class="plbls-toggle__piste"></span>
          <span class="plbls-toggle__texte">Transcrire automatiquement mes messages vocaux{#if app.transcription === 'alpha'} <span class="plbls-badge">ALPHA</span>{/if}</span>
        </label>{/if}
        <div class="plbls-field">
          <label class="plbls-field__label" for="density">Densité des messages</label>
          <select id="density" class="plbls-select" bind:value={prefs.density} onchange={() => prefs.save()}>
            <option value={'confortable' satisfies Density}>Confortable</option>
            <option value={'compacte' satisfies Density}>Compacte</option>
          </select>
        </div>

        <div class="plbls-field">
          <label class="plbls-field__label" for="theme">Thème</label>
          <select id="theme" class="plbls-select" bind:value={prefs.theme} onchange={() => prefs.save()}>
            <option value={'auto' satisfies Theme}>Système</option>
            <option value={'light' satisfies Theme}>Clair</option>
            <option value={'dark' satisfies Theme}>Sombre</option>
          </select>
        </div>

      {:else if tab === 'voice'}
        <div class="plbls-field">
          <label class="plbls-field__label" for="mic">Micro</label>
          <select id="mic" class="plbls-select" title={pick(prefs.micId)} bind:value={prefs.micId} onchange={() => change('audioinput')}>
            <option value="">Par défaut</option>
            {#each mics as d, i (d.deviceId)}<option value={d.deviceId}>{label(d, i)}</option>{/each}
          </select>
        </div>
        {#if canSink}
          <div class="plbls-field">
            <label class="plbls-field__label" for="spk">Sortie audio</label>
            <select id="spk" class="plbls-select" title={pick(prefs.speakerId)} bind:value={prefs.speakerId} onchange={() => change('audiooutput')}>
              <option value="">Par défaut</option>
              {#each speakers as d, i (d.deviceId)}<option value={d.deviceId}>{label(d, i)}</option>{/each}
            </select>
          </div>
        {/if}
        <div class="plbls-field">
          <label class="plbls-field__label" for="cam">Caméra</label>
          <select id="cam" class="plbls-select" title={pick(prefs.cameraId)} bind:value={prefs.cameraId} onchange={() => change('videoinput')}>
            <option value="">Par défaut</option>
            {#each cams as d, i (d.deviceId)}<option value={d.deviceId}>{label(d, i)}</option>{/each}
          </select>
        </div>
        <div class="plbls-field">
          <label class="plbls-field__label" for="gain">Volume du micro : {prefs.micGain} %</label>
          <input id="gain" type="range" min="0" max="300" step="5" bind:value={prefs.micGain} oninput={() => { void voice.setMicGain(); micTest.refresh() }} onchange={() => prefs.save()} />
        </div>
        <div class="plbls-field">
          <label class="plbls-field__label" for="noise">Réduction de bruit</label>
          <select id="noise" class="plbls-select" bind:value={prefs.noise} onchange={noiseChanged}>
            <option value={'off' satisfies NoiseMode}>Désactivée</option>
            <option value={'standard' satisfies NoiseMode}>Standard (navigateur)</option>
            <option value={'strong' satisfies NoiseMode}>Renforcée (clavier, ventilateur…)</option>
          </select>
        </div>
        <div class="plbls-field">
          <label class="plbls-field__label" for="micmode">Activation du micro</label>
          <select id="micmode" class="plbls-select" value={micMode} onchange={(e) => setMicMode(e.currentTarget.value)}>
            <option value="always">Toujours ouvert</option>
            <option value="vad">Détection de voix</option>
            <option value="ptt">Push-to-talk (touche à maintenir)</option>
          </select>
        </div>
        {#if prefs.vad && !prefs.ptt}
          <p class="muted small">Le micro n'émet que lorsque ta voix dépasse le seuil, réglable ici avec la jauge du test ou dans le salon vocal.</p>
        {/if}
        <div class="plbls-field mictest">
          <span class="plbls-field__label">Tester mon micro</span>
          <div class="row">
            {#if micTest.live}
              <button class="plbls-button plbls-button--compact" onclick={() => micTest.stop()}>Arrêter l'écoute</button>
            {:else}
              <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={micTest.recording || voice.state !== 'idle'} onclick={() => void micTest.startLive()}>M'écouter en direct</button>
            {/if}
            {#if micTest.recording}
              <button class="plbls-button plbls-button--compact" onclick={() => micTest.stopRecording()}>Terminer ({Math.round(micTest.elapsed)} / {MIC_TEST_MAX_S} s)</button>
            {:else}
              <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={voice.state !== 'idle'} onclick={() => void micTest.record()}>M'enregistrer</button>
            {/if}
          </div>
          {#if voice.state !== 'idle'}<p class="muted small">Quitte le salon vocal pour tester ton micro.</p>{/if}
          {#if micTest.live}<p class="muted small">Sans casque, tu risques un écho : baisse le son ou mets un casque.</p>{/if}
          {#if micTest.recording && prefs.ptt}<p class="muted small">Push-to-talk : maintiens <kbd>{prefs.pttKey.replace(/^Key/, '')}</kbd> pour enregistrer.</p>{/if}
          {#if micTest.error}<p class="small" role="alert">{micTest.error}</p>{/if}
          {#if micTest.live || micTest.recording || voice.state !== 'idle'}
            {@const inCall = voice.state !== 'idle'}
            {@const lvl = inCall ? voice.level : micTest.level}
            {@const act = inCall ? voice.level > prefs.vadThreshold : micTest.active}
            <div class="meter" role="meter" aria-label="Niveau du micro" aria-valuemin="-80" aria-valuemax="0" aria-valuenow={Math.max(-80, lvl)}>
              <div class="meter-fill" class:over={act} class:under={!act} style:width="{dbPct(lvl)}%"></div>
              <div class="meter-mark" style:left="{dbPct(prefs.vadThreshold)}%"></div>
            </div>
            <label class="small muted">Seuil de voix : {prefs.vadThreshold} dB · {act ? 'voix détectée' : 'silence'}
              <input type="range" min="-75" max="-15" step="1" bind:value={prefs.vadThreshold} onchange={() => prefs.save()} aria-label="Seuil de détection de voix" />
            </label>
          {/if}
          {#if micTest.clip}
            <div class="row">
              {#each [['off', 'Désactivée'], ['standard', 'Standard'], ['strong', 'Renforcée']] as const as [m, l] (m)}
                <button class="plbls-button plbls-button--compact" class:plbls-button--contour={micTest.playing !== m} onclick={() => (micTest.playing === m ? micTest.stop() : micTest.play(m))}>{micTest.playing === m ? 'Stop' : 'Réécouter'} · {l}</button>
              {/each}
            </div>
            <p class="muted small">Même enregistrement, trois niveaux de réduction de bruit. Il reste sur cet appareil.</p>
          {/if}
        </div>
        {#if prefs.ptt}
          <div class="row">
            <span class="small">Touche : <kbd>{prefs.pttKey.replace(/^Key/, '')}</kbd></span>
            {#if capturing}
              <input class="plbls-input" readonly placeholder="Appuie sur une touche…" onkeydown={captureKey} onblur={() => (capturing = false)} {@attach (n) => n.focus()} />
            {:else}
              <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (capturing = true)}>Changer</button>
            {/if}
          </div>
          <p class="muted small">{platform.pttNote}</p>
        {/if}
        {#if Ptt}<Ptt />{/if}
        {#if Player}<Player />{/if}
        <label class="plbls-toggle">
          <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.watchAuto} onchange={() => { prefs.watchAutoSet = true; prefs.save() }} />
          <span class="plbls-toggle__piste"></span>
          <span class="plbls-toggle__texte">Regarder automatiquement les streams (sinon : bouton « Regarder » sur chaque stream)</span>
        </label>
        <label class="plbls-toggle">
          <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.voiceSounds} onchange={() => prefs.save()} />
          <span class="plbls-toggle__piste"></span>
          <span class="plbls-toggle__texte">Sons d'arrivée et de départ dans le vocal</span>
        </label>
        {#if prefs.voiceSounds}
          {#each [['join', 'Son d\'arrivée'], ['leave', 'Son de départ']] as [kind, title] (kind)}
            {@const k = kind as 'join' | 'leave'}
            <div class="plbls-field">
              <label class="plbls-field__label" for="snd-{k}">{title}</label>
              <div class="row">
                <select id="snd-{k}" class="plbls-select" value={k === 'join' ? prefs.joinSound : prefs.leaveSound}
                  onchange={(e) => { const v = e.currentTarget.value as SoundChoice; if (k === 'join') prefs.joinSound = v; else prefs.leaveSound = v; prefs.save(); playTone(k) }}>
                  {#each Object.entries(SOUND_PRESETS) as [id, s] (id)}<option value={id}>{s.label}</option>{/each}
                  {#if (k === 'join' ? prefs.customJoin : prefs.customLeave)}<option value="custom">Mon fichier</option>{/if}
                  <option value="off">Aucun</option>
                </select>
                <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => playTone(k)}>Écouter</button>
                <label class="plbls-button plbls-button--compact plbls-button--contour">Mon fichier…<input type="file" accept="audio/*" hidden onchange={pickSound(k)} /></label>
              </div>
            </div>
          {/each}
          <div class="plbls-field">
            <label class="plbls-field__label" for="snd-vol">Volume des sons : {prefs.soundVolume} %</label>
            <input id="snd-vol" type="range" min="0" max="100" step="5" bind:value={prefs.soundVolume} onchange={() => { prefs.save(); playTone('join') }} />
          </div>
        {/if}
        <label class="plbls-toggle">
          <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.screenAudio} onchange={() => prefs.save()} />
          <span class="plbls-toggle__piste"></span>
          <span class="plbls-toggle__texte">Partager aussi le son de l'écran (si le navigateur le permet)</span>
        </label>
        <div class="plbls-field">
          <label class="plbls-field__label" for="cq">Qualité de la caméra</label>
          <select id="cq" class="plbls-select" bind:value={prefs.cameraQuality} onchange={restartCamera}>
            <option value={'360p' satisfies CameraQuality}>360p (connexion faible)</option>
            <option value={'720p' satisfies CameraQuality}>720p (recommandé)</option>
            <option value={'1080p' satisfies CameraQuality}>1080p (connexion rapide)</option>
          </select>
        </div>
        <div class="plbls-field">
          <label class="plbls-field__label" for="codec">Codec vidéo</label>
          <select id="codec" class="plbls-select" bind:value={prefs.videoCodec} onchange={restartCamera}>
            <option value={'auto' satisfies VideoCodecPref}>Automatique (VP8, le plus compatible)</option>
            <option value={'vp9' satisfies VideoCodecPref}>VP9 (plus net à débit égal, plus de processeur)</option>
            <option value={'h264' satisfies VideoCodecPref}>H.264 (accélération matérielle)</option>
          </select>
        </div>
        <h3 class="set-title">Arrière-plan de la caméra</h3>
        {#if !voice.camera}<CameraPreview />{/if}
        <CameraFxControls />
        <CameraOverlayControls />
        {#if Share}<Share />{/if}
        <label class="plbls-toggle">
          <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.shareListening} onchange={() => prefs.save()} />
          <span class="plbls-toggle__piste"></span>
          <span class="plbls-toggle__texte">Afficher ce que j'écoute (« 🎵 Écoute … » dans l'activité ; personne ne le voit sinon)</span>
        </label>
        <div class="plbls-field">
          <label class="plbls-field__label" for="sq">Résolution du partage d'écran</label>
          <select id="sq" class="plbls-select" bind:value={prefs.screenRes} onchange={() => prefs.save()}>
            <option value={'720p' satisfies ScreenRes}>720p</option>
            <option value={'1080p' satisfies ScreenRes}>1080p</option>
            <option value={'1440p' satisfies ScreenRes}>1440p</option>
            <option value={'2160p' satisfies ScreenRes}>4K</option>
          </select>
        </div>
        <div class="plbls-field">
          <label class="plbls-field__label" for="sf">Images par seconde du partage</label>
          <select id="sf" class="plbls-select" bind:value={prefs.screenFps} onchange={() => prefs.save()}>
            <option value={30}>30</option>
            <option value={60}>60</option>
            <option value={120}>120</option>
          </select>
        </div>

      {:else}
        {#if Update}<Update />{/if}
      {/if}
    </section>
  </div>
</Modal>
