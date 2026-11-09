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
  import Icon from './Icon.svelte'
  import { voice } from '../blocks/vocal/voice.svelte'
  import { micTest, MIC_TEST_MAX_S } from '../lib/mictest.svelte'
  import { onDestroy } from 'svelte'
  import { logout } from '../lib/auth'
  import { notify } from '../lib/ui'
  import { platform } from '../lib/platform'
  import { updater } from '../lib/updater.svelte'
  import { settingsNav, searchSettings, SETTINGS_ICONS, SETTINGS_TABS, type SettingsTab } from '../lib/settings.svelte'
  import SettingsNotifications from '../blocks/notifications/SettingsNotifications.svelte'
  import SettingsDebug from './SettingsDebug.svelte'
  import SettingsReports from '../blocks/signalements/SettingsReports.svelte'

  const { settingsUpdate: Update, settingsPtt: Ptt, settingsShare: Share, settingsVoice: Player } = platform.slots
  import { transcriptionOn } from '../blocks/transcription/transcript'

  let { onclose }: { onclose: () => void } = $props()

  // Onglet demandé (lien depuis un message), sinon le dernier ouvert. Sur mobile, on commence par la liste des catégories.
  const asked = settingsNav.request
  settingsNav.request = ''
  const valid = (t: string): t is SettingsTab => SETTINGS_TABS.some((x) => x.id === t)
  let tab = $state<SettingsTab>(valid(asked) ? asked : valid(prefs.settingsTab) ? prefs.settingsTab : 'account')
  let view = $state<'list' | 'page'>(asked || !(typeof matchMedia !== 'undefined' && matchMedia('(max-width: 700px)').matches) ? 'page' : 'list')
  let query = $state('')
  const found = $derived(searchSettings(query))
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

  function keydown(e: KeyboardEvent) {
    if (e.key !== 'Escape' || e.defaultPrevented) return
    const t = e.target as HTMLElement | null
    if (t instanceof HTMLInputElement && t.type === 'search' && t.value) return
    if (document.querySelector('plbls-modal, [aria-modal="true"], [role="menu"]')) return
    e.preventDefault()
    onclose()
  }

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

  const THEMES = [['auto', 'Système', 'suit l’appareil'], ['light', 'Clair', 'Papier'], ['dark', 'Sombre', 'Fusain']] as const satisfies readonly (readonly [Theme, string, string])[]
  const DENSITIES = [['confortable', 'Confortable'], ['compacte', 'Compacte']] as const satisfies readonly (readonly [Density, string])[]
</script>

<svelte:window onkeydown={keydown} />

<div class="parametres" role="dialog" aria-modal="false" aria-label="Paramètres" data-view={view} tabindex="-1" {@attach (n) => n.focus({ preventScroll: true })}>
  <nav class="p-nav" aria-label="Catégories de réglages">
    <div class="p-nav-h">
      <span class="p-nav-titre">Paramètres</span>
      <button class="v-ib p-nav-fermer" aria-label="Fermer les paramètres" onclick={onclose}><Icon name="x" /></button>
    </div>
    <div class="p-cherche">
      <Icon name="cherche" size={16} />
      <input type="search" placeholder="Chercher un réglage" aria-label="Chercher un réglage" bind:value={query} />
    </div>
    <div class="p-onglets">
      {#each found as t (t.id)}
        <button class="p-onglet" class:on={tab === t.id} aria-current={tab === t.id ? 'page' : undefined} onclick={() => go(t.id)}>
          <Icon name={SETTINGS_ICONS[t.id]} size={18} />
          <span class="p-onglet-txt"><span>{t.label}</span>{#if t.hits.length}<span class="p-onglet-hits">{t.hits.slice(0, 3).join(' · ')}</span>{/if}</span>
        </button>
      {:else}
        <p class="v-note p-vide">Aucun réglage ne correspond.</p>
      {/each}
    </div>
    {#if updater.version}<span class="p-version v-mono">Estompe {updater.version}</span>{/if}
  </nav>

  <div class="p-page">
    <header class="p-top">
      <button class="plbls-button plbls-button--compact plbls-button--discret p-retour" onclick={() => (view = 'list')}>‹ Paramètres</button>
      <h2 class="p-titre" id="set-h">{SETTINGS_TABS.find((t) => t.id === tab)?.label}</h2>
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={onclose}><Icon name="x" size={16} />Fermer</button>
    </header>

    <div class="p-corps" aria-labelledby="set-h" role="region">
      {#if tab === 'account'}
        <section class="p-compte">
          <Avatar pseudo={app.pseudo} userId={app.meId} size="lg" />
          <div class="p-compte-txt">
            <span class="v-note">Connecté en tant que</span>
            <strong class="p-pseudo">{app.pseudo}</strong>
            {#if app.peut('profile.avatar')}<div class="row">
              <label class="plbls-button plbls-button--compact" class:is-busy={avatarBusy}>
                <Icon name="envoie" size={16} />Changer l'avatar<input type="file" accept="image/*" hidden onchange={pickAvatar} disabled={avatarBusy} />
              </label>
              <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={removeAvatar} disabled={avatarBusy}>Supprimer</button>
            </div>{/if}
          </div>
        </section>
        {#if app.servers.length}
          <section class="p-section">
            <h3 class="p-sec">Photo de profil par serveur</h3>
            <p class="v-note">Sans photo propre à un serveur, ta photo globale y est utilisée.</p>
            <div class="p-carte">
              {#each app.servers as sv (sv.id)}
                <div class="p-ligne">
                  <Avatar pseudo={app.pseudo} userId={app.meId} serverId={sv.id} size="sm" />
                  <strong class="p-grow truncate">{sv.name}</strong>
                  <label class="plbls-button plbls-button--compact plbls-button--contour" class:is-busy={serverAvatarBusy === sv.id}>
                    Changer<input type="file" accept="image/*" hidden onchange={(e) => pickServerAvatar(sv.id, e)} disabled={serverAvatarBusy === sv.id} />
                  </label>
                  {#if avatars.serverVersion(app.meId, sv.id)}
                    <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => removeServerAvatar(sv.id)} disabled={serverAvatarBusy === sv.id}>Retirer</button>
                  {/if}
                </div>
              {/each}
            </div>
          </section>
        {/if}
        <section class="p-ligne p-session">
          <h3 class="p-sec p-grow">Session</h3>
          <button class="plbls-button plbls-button--contour plbls-button--danger" onclick={() => logout()}><Icon name="sortie" size={16} />Se déconnecter</button>
        </section>

      {:else if tab === 'notifications'}
        <SettingsNotifications />

      {:else if tab === 'debug'}
        <SettingsDebug />

      {:else if tab === 'reports'}
        {#if app.on('signalements') && app.peut('reports.create')}<SettingsReports />{/if}

      {:else if tab === 'appearance'}
        <fieldset class="p-section">
          <legend class="p-sec">Thème</legend>
          <div class="p-choix p-choix-3">
            {#each THEMES as [id, nom, note] (id)}
              <label class="p-carte-choix" class:on={prefs.theme === id}>
                <span class="p-apercu p-apercu-{id}" aria-hidden="true"><i></i><i></i><i></i></span>
                <span class="p-choix-pied">
                  <input type="radio" name="theme" value={id} bind:group={prefs.theme} onchange={() => prefs.save()} />
                  <strong class="p-grow">{nom}</strong><span class="v-note">{note}</span>
                </span>
              </label>
            {/each}
          </div>
        </fieldset>
        <fieldset class="p-section">
          <legend class="p-sec">Densité des messages</legend>
          <div class="p-choix">
            {#each DENSITIES as [id, nom] (id)}
              <label class="p-carte-choix p-densite-{id}" class:on={prefs.density === id}>
                <span class="p-choix-pied">
                  <input type="radio" name="density" value={id} bind:group={prefs.density} onchange={() => prefs.save()} />
                  <strong>{nom}</strong>
                </span>
                <span class="p-lignes" aria-hidden="true"><i></i><i></i><i></i></span>
              </label>
            {/each}
          </div>
        </fieldset>
        <div class="p-carte">
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
          <label class="plbls-toggle">
            <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.reduceMotion} onchange={() => prefs.save()} aria-describedby="p-motion-aide" />
            <span class="plbls-toggle__piste"></span>
            <span class="plbls-toggle__texte">Réduire les animations</span>
          </label>
          <p class="v-note p-aide" id="p-motion-aide">{prefs.reduceMotion ? 'Activé : animations réduites dans Estompe, quel que soit le réglage du système.' : 'Désactivé : suit le réglage du système.'}</p>
        </div>

      {:else if tab === 'voice'}
        <div class="p-grille">
          <div class="p-col">
            <section class="p-section">
              <h3 class="p-sec">Micro et sortie</h3>
              <div class="p-carte">
                <div class="p-ligne">
                  <label class="p-grow" for="mic">Micro</label>
                  <select id="mic" class="plbls-select" title={pick(prefs.micId)} bind:value={prefs.micId} onchange={() => change('audioinput')}>
                    <option value="">Par défaut</option>
                    {#each mics as d, i (d.deviceId)}<option value={d.deviceId}>{label(d, i)}</option>{/each}
                  </select>
                </div>
                {#if canSink}
                  <div class="p-ligne">
                    <label class="p-grow" for="spk">Sortie audio</label>
                    <select id="spk" class="plbls-select" title={pick(prefs.speakerId)} bind:value={prefs.speakerId} onchange={() => change('audiooutput')}>
                      <option value="">Par défaut</option>
                      {#each speakers as d, i (d.deviceId)}<option value={d.deviceId}>{label(d, i)}</option>{/each}
                    </select>
                  </div>
                {/if}
                <div class="p-ligne">
                  <label class="p-grow" for="cam">Caméra</label>
                  <select id="cam" class="plbls-select" title={pick(prefs.cameraId)} bind:value={prefs.cameraId} onchange={() => change('videoinput')}>
                    <option value="">Par défaut</option>
                    {#each cams as d, i (d.deviceId)}<option value={d.deviceId}>{label(d, i)}</option>{/each}
                  </select>
                </div>
                <div class="p-ligne">
                  <label class="p-grow" for="gain">Volume du micro : <span class="v-mono">{prefs.micGain} %</span></label>
                  <input id="gain" type="range" min="0" max="300" step="5" bind:value={prefs.micGain} oninput={() => { void voice.setMicGain(); micTest.refresh() }} onchange={() => prefs.save()} />
                </div>
                <div class="p-ligne">
                  <label class="p-grow" for="noise">Réduction de bruit</label>
                  <select id="noise" class="plbls-select" bind:value={prefs.noise} onchange={noiseChanged}>
                    <option value={'off' satisfies NoiseMode}>Désactivée</option>
                    <option value={'standard' satisfies NoiseMode}>Standard (navigateur)</option>
                    <option value={'strong' satisfies NoiseMode}>Renforcée (clavier, ventilateur…)</option>
                  </select>
                </div>
                <div class="p-ligne">
                  <div class="p-grow">
                    <label for="micmode">Activation du micro</label>
                    {#if prefs.vad && !prefs.ptt}
                      <p class="v-note">Le micro n'émet que lorsque ta voix dépasse le seuil, réglable ici avec la jauge du test ou dans le salon vocal.</p>
                    {/if}
                  </div>
                  <select id="micmode" class="plbls-select" value={micMode} onchange={(e) => setMicMode(e.currentTarget.value)}>
                    <option value="always">Toujours ouvert</option>
                    <option value="vad">Détection de voix</option>
                    <option value="ptt">Push-to-talk (touche à maintenir)</option>
                  </select>
                </div>
                {#if prefs.ptt}
                  <div class="p-ligne">
                    <div class="p-grow">
                      <span>Touche : <kbd>{prefs.pttKey.replace(/^Key/, '')}</kbd></span>
                      <p class="v-note">{platform.pttNote}</p>
                    </div>
                    {#if capturing}
                      <input class="plbls-input p-touche" readonly placeholder="Appuie sur une touche…" onkeydown={captureKey} onblur={() => (capturing = false)} {@attach (n) => n.focus()} />
                    {:else}
                      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (capturing = true)}>Changer</button>
                    {/if}
                  </div>
                {/if}
              </div>
            </section>

            <section class="p-section mictest">
              <h3 class="p-sec">Tester mon micro</h3>
              <div class="p-carte p-carte-pleine">
                <div class="row">
                  {#if micTest.live}
                    <button class="plbls-button plbls-button--compact" onclick={() => micTest.stop()}>Arrêter l'écoute</button>
                  {:else}
                    <button class="plbls-button plbls-button--compact" disabled={micTest.recording || voice.state !== 'idle'} onclick={() => void micTest.startLive()}><Icon name="casque" size={16} />M'écouter en direct</button>
                  {/if}
                  {#if micTest.recording}
                    <button class="plbls-button plbls-button--compact" onclick={() => micTest.stopRecording()}>Terminer ({Math.round(micTest.elapsed)} / {MIC_TEST_MAX_S} s)</button>
                  {:else}
                    <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={voice.state !== 'idle'} onclick={() => void micTest.record()}><span class="p-rec" aria-hidden="true"></span>M'enregistrer</button>
                  {/if}
                </div>
                {#if voice.state !== 'idle'}<p class="v-note">Quitte le salon vocal pour tester ton micro.</p>{/if}
                {#if micTest.live}<p class="v-note">Sans casque, tu risques un écho : baisse le son ou mets un casque.</p>{/if}
                {#if micTest.recording && prefs.ptt}<p class="v-note">Push-to-talk : maintiens <kbd>{prefs.pttKey.replace(/^Key/, '')}</kbd> pour enregistrer.</p>{/if}
                {#if micTest.error}<p class="small p-erreur" role="alert">{micTest.error}</p>{/if}
                {#if micTest.live || micTest.recording || voice.state !== 'idle'}
                  {@const inCall = voice.state !== 'idle'}
                  {@const lvl = inCall ? voice.level : micTest.level}
                  {@const act = inCall ? voice.level > prefs.vadThreshold : micTest.active}
                  <div class="meter" role="meter" aria-label="Niveau du micro" aria-valuemin="-80" aria-valuemax="0" aria-valuenow={Math.max(-80, lvl)}>
                    <div class="meter-fill" class:over={act} class:under={!act} style:width="{dbPct(lvl)}%"></div>
                    <div class="meter-mark" style:left="{dbPct(prefs.vadThreshold)}%"></div>
                  </div>
                  <label class="p-seuil v-note">
                    <span class="row"><span class="p-grow">Seuil de voix : <b class="v-mono">{prefs.vadThreshold} dB</b></span><span class="v-tag" class:ok={act}>{#if act}<Icon name="onde" size={14} />{/if}{act ? 'voix détectée' : 'silence'}</span></span>
                    <input type="range" min="-75" max="-15" step="1" bind:value={prefs.vadThreshold} onchange={() => prefs.save()} aria-label="Seuil de détection de voix" />
                  </label>
                {/if}
                {#if micTest.clip}
                  <span class="v-lbl">Réécouter le même enregistrement</span>
                  <div class="row">
                    {#each [['off', 'Désactivée'], ['standard', 'Standard'], ['strong', 'Renforcée']] as const as [m, l] (m)}
                      <button class="plbls-button plbls-button--compact" class:plbls-button--contour={micTest.playing !== m} class:v-rose={micTest.playing === m} onclick={() => (micTest.playing === m ? micTest.stop() : micTest.play(m))}>{micTest.playing === m ? 'Stop' : 'Réécouter'} · {l}</button>
                    {/each}
                  </div>
                  <p class="v-note">Même enregistrement, trois niveaux de réduction de bruit. Il reste sur cet appareil.</p>
                {/if}
              </div>
            </section>

            {#if Ptt || Player}
              <div class="p-carte">
                {#if Ptt}<Ptt />{/if}
                {#if Player}<Player />{/if}
              </div>
            {/if}
          </div>

          <div class="p-col">
            <section class="p-section">
              <h3 class="p-sec">Sons du vocal</h3>
              <div class="p-carte">
                <label class="plbls-toggle">
                  <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.voiceSounds} onchange={() => prefs.save()} />
                  <span class="plbls-toggle__piste"></span>
                  <span class="plbls-toggle__texte">Sons d'arrivée et de départ dans le vocal</span>
                </label>
                {#if prefs.voiceSounds}
                  {#each [['join', 'Son d\'arrivée'], ['leave', 'Son de départ']] as [kind, title] (kind)}
                    {@const k = kind as 'join' | 'leave'}
                    <div class="p-ligne p-ligne-son">
                      <label class="p-grow" for="snd-{k}">{title}</label>
                      <select id="snd-{k}" class="plbls-select" value={k === 'join' ? prefs.joinSound : prefs.leaveSound}
                        onchange={(e) => { const v = e.currentTarget.value as SoundChoice; if (k === 'join') prefs.joinSound = v; else prefs.leaveSound = v; prefs.save(); playTone(k) }}>
                        {#each Object.entries(SOUND_PRESETS) as [id, s] (id)}<option value={id}>{s.label}</option>{/each}
                        {#if (k === 'join' ? prefs.customJoin : prefs.customLeave)}<option value="custom">Mon fichier</option>{/if}
                        <option value="off">Aucun</option>
                      </select>
                      <span class="row">
                        <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => playTone(k)}><Icon name="lecture" size={14} />Écouter</button>
                        <label class="plbls-button plbls-button--compact plbls-button--contour">Mon fichier…<input type="file" accept="audio/*" hidden onchange={pickSound(k)} /></label>
                      </span>
                    </div>
                  {/each}
                  <div class="p-ligne">
                    <label class="p-grow" for="snd-vol">Volume des sons : <span class="v-mono">{prefs.soundVolume} %</span></label>
                    <input id="snd-vol" type="range" min="0" max="100" step="5" bind:value={prefs.soundVolume} onchange={() => { prefs.save(); playTone('join') }} />
                  </div>
                {/if}
                <label class="plbls-toggle">
                  <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.watchAuto} onchange={() => { prefs.watchAutoSet = true; prefs.save() }} />
                  <span class="plbls-toggle__piste"></span>
                  <span class="plbls-toggle__texte">Regarder automatiquement les streams (sinon : bouton « Regarder » sur chaque stream)</span>
                </label>
              </div>
            </section>

            <section class="p-section">
              <h3 class="p-sec">Caméra et partage d'écran</h3>
              <div class="p-carte">
                <div class="p-ligne">
                  <label class="p-grow" for="cq">Qualité de la caméra</label>
                  <select id="cq" class="plbls-select" bind:value={prefs.cameraQuality} onchange={restartCamera}>
                    <option value={'360p' satisfies CameraQuality}>360p (connexion faible)</option>
                    <option value={'720p' satisfies CameraQuality}>720p (recommandé)</option>
                    <option value={'1080p' satisfies CameraQuality}>1080p (connexion rapide)</option>
                  </select>
                </div>
                <div class="p-ligne">
                  <label class="p-grow" for="codec">Codec vidéo</label>
                  <select id="codec" class="plbls-select" bind:value={prefs.videoCodec} onchange={restartCamera}>
                    <option value={'auto' satisfies VideoCodecPref}>Automatique (VP8, le plus compatible)</option>
                    <option value={'vp9' satisfies VideoCodecPref}>VP9 (plus net à débit égal, plus de processeur)</option>
                    <option value={'h264' satisfies VideoCodecPref}>H.264 (accélération matérielle)</option>
                  </select>
                </div>
                <div class="p-ligne">
                  <label class="p-grow" for="sq">Résolution du partage d'écran</label>
                  <select id="sq" class="plbls-select" bind:value={prefs.screenRes} onchange={() => prefs.save()}>
                    <option value={'720p' satisfies ScreenRes}>720p</option>
                    <option value={'1080p' satisfies ScreenRes}>1080p</option>
                    <option value={'1440p' satisfies ScreenRes}>1440p</option>
                    <option value={'2160p' satisfies ScreenRes}>4K</option>
                  </select>
                </div>
                <div class="p-ligne">
                  <label class="p-grow" for="sf">Images par seconde du partage</label>
                  <select id="sf" class="plbls-select" bind:value={prefs.screenFps} onchange={() => prefs.save()}>
                    <option value={30}>30</option>
                    <option value={60}>60</option>
                    <option value={120}>120</option>
                  </select>
                </div>
                <label class="plbls-toggle">
                  <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.screenAudio} onchange={() => prefs.save()} />
                  <span class="plbls-toggle__piste"></span>
                  <span class="plbls-toggle__texte">Partager aussi le son de l'écran (si le navigateur le permet)</span>
                </label>
                <label class="plbls-toggle">
                  <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.shareListening} onchange={() => prefs.save()} />
                  <span class="plbls-toggle__piste"></span>
                  <span class="plbls-toggle__texte">Afficher ce que j'écoute (« 🎵 Écoute … » dans l'activité ; personne ne le voit sinon)</span>
                </label>
                {#if Share}<Share />{/if}
              </div>
            </section>

            <section class="p-section">
              <h3 class="p-sec">Arrière-plan de la caméra</h3>
              <div class="p-carte">
                {#if !voice.camera}<div class="p-apercu-cam"><CameraPreview /></div>{/if}
                <CameraFxControls />
                <CameraOverlayControls />
              </div>
            </section>
          </div>
        </div>

      {:else if Update}
        <div class="p-carte p-carte-pleine">
          <Update />
        </div>
      {/if}
    </div>
  </div>
</div>
