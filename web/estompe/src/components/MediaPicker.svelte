<script lang="ts">
  import Loader from './Loader.svelte'
  import Modal from './Modal.svelte'
  import CameraPreview from './CameraPreview.svelte'
  import CameraFxControls from './CameraFxControls.svelte'
  import { platform, type CaptureSource } from '../lib/platform'
  import { voice, SCREEN_RES, SCREEN_FPS, CAMERA, screenBitrate, clampScreen } from '../blocks/vocal/voice.svelte'
  import { prefs, type ScreenRes, type ScreenFps, type CameraQuality } from '../lib/prefs.svelte'

  const RES_LABEL: Record<ScreenRes, string> = { '720p': '720p', '1080p': '1080p', '1440p': '1440p', '2160p': '4K' }
  const CAMERA_LABEL: Record<CameraQuality, string> = { '360p': '360p (connexion faible)', '720p': '720p (recommandé)', '1080p': '1080p (connexion rapide)' }
  const PRESETS: { id: string; label: string; hint: string; res: ScreenRes; fps: ScreenFps }[] = [
    { id: 'fluide', label: 'Fluide', hint: 'Jeux et vidéos : mouvement fluide', res: '1080p', fps: 60 },
    { id: 'net', label: 'Net', hint: 'Documents et code : image nette', res: '1440p', fps: 30 },
    { id: 'eco', label: 'Économie de données', hint: 'Connexion lente ou mobile', res: '720p', fps: 30 },
  ]
  const preset = (p: (typeof PRESETS)[number]) => { prefs.screenRes = p.res; prefs.screenFps = p.fps }
  const mbps = (bits: number) => `${(bits / 1e6).toFixed(bits < 1e7 ? 1 : 0).replace('.0', '')} Mb/s`

  // Choix effectif : ramené aux plafonds du serveur (affichés grisés dans les listes).
  const eff = $derived(clampScreen(prefs.screenRes, prefs.screenFps, voice.maxScreen))
  const resOk = (r: ScreenRes) => SCREEN_RES[r].height <= voice.maxScreen.height
  const resNote = (r: ScreenRes) => resOk(r) ? '' : SCREEN_RES[r].height > voice.limits.height ? ' (non proposé par le serveur)' : ' (au-delà de ton écran)'
  const kind = $derived(voice.picker)
  const close = () => voice.closePicker()
  let shot = $state<HTMLVideoElement>() // aperçu de l'écran choisi
  $effect(() => { if (shot) shot.srcObject = voice.preview })
  // Résolution et images/s s'appliquent à l'aperçu pour qu'il les reflète.
  $effect(() => { void eff; void voice.tunePreview() })

  // --- caméra : liste (l'aperçu en direct est CameraPreview) ---
  let cams = $state<MediaDeviceInfo[]>([])

  // Partage déjà en cours : la fenêtre ne règle que la qualité, appliquée à la volée.
  const live = $derived(kind === 'screen' && voice.screen)

  // --- liste maison des écrans et fenêtres (app Windows) ---
  const sourceStep = $derived(kind === 'screen' && !live && voice.step === 'source')
  let sources = $state<CaptureSource[]>([])
  let thumbs = $state<Record<string, string>>({})
  let tab = $state<'screen' | 'window'>('screen')
  let loadError = $state('')
  let loading = $state(false)
  let round = 0

  async function loadSources() {
    const mine = ++round
    loading = true
    loadError = ''
    try {
      const list = await platform.capture!.listSources()
      if (mine !== round) return
      sources = list
      const queue = [...list]
      // Miniatures chargées au fil de l'eau, quelques-unes à la fois.
      await Promise.all(Array.from({ length: 3 }, async () => {
        for (let s = queue.shift(); s && mine === round; s = queue.shift()) {
          const id = s.id
          const url = await platform.capture!.thumbUrl(id).catch(() => '')
          if (mine !== round) return URL.revokeObjectURL(url)
          if (url) thumbs[id] = url
        }
      }))
    } catch {
      if (mine === round) loadError = "Impossible de lister les écrans et fenêtres."
    } finally {
      if (mine === round) loading = false
    }
  }

  $effect(() => {
    if (!sourceStep) return
    void loadSources()
    return () => { round++; Object.values(thumbs).forEach(URL.revokeObjectURL); thumbs = {} }
  })

  async function start() {
    const k = kind
    prefs.save()
    if (live) { close(); return void voice.setScreenQuality() }
    if (k === 'screen') return void voice.publishScreen()
    close() // l'aperçu s'arrête avant que la caméra soit publiée (une caméra ne s'ouvre pas deux fois partout)
    if (k === 'camera') await voice.toggleCamera()
  }
</script>

{#if kind}
  <Modal title={live ? 'Qualité du partage' : kind === 'screen' ? "Partager l'écran" : 'Activer la caméra'} onclose={close}>
    <div class="picker" class:wide={sourceStep}>
      {#if sourceStep}
        <div class="src-tabs" role="tablist">
          <button role="tab" aria-selected={tab === 'screen'} class:sel={tab === 'screen'} onclick={() => (tab = 'screen')}>Écrans</button>
          <button role="tab" aria-selected={tab === 'window'} class:sel={tab === 'window'} onclick={() => (tab = 'window')}>Fenêtres</button>
        </div>
        <div class="src-grid">
          {#each sources.filter((s) => s.kind === tab) as s (s.id)}
            <button class="src-card" onclick={() => voice.chooseSource(s)} title={s.title}>
              <span class="src-thumb">{#if thumbs[s.id]}<img src={thumbs[s.id]} alt="" />{:else}<Loader size="sm" />{/if}</span>
              <strong class="truncate">{s.title}</strong>
              <span class="small muted truncate">{s.app || `${s.width} × ${s.height}`}</span>
            </button>
          {:else}
            {#if !loading}<p class="muted small">{tab === 'screen' ? 'Aucun écran trouvé.' : 'Aucune fenêtre trouvée.'}</p>{/if}
          {/each}
        </div>
        {#if loadError || voice.error}<p class="picker-err-line small" role="alert">{loadError || voice.error}</p>{/if}
        <div class="row picker-actions">
          <button class="plbls-button plbls-button--contour" onclick={() => void voice.pickScreen('system')} title="Si la liste ne montre pas ce que tu cherches">Autre méthode</button>
          <button class="plbls-button plbls-button--contour" onclick={() => void loadSources()}>Actualiser</button>
          <button class="plbls-button plbls-button--contour" onclick={close}>Annuler</button>
        </div>
      {:else if kind === 'screen'}
        {#if !live}
          {#if voice.nativeSource}
            <p class="small"><strong>{voice.nativeSource.title}</strong> · {voice.nativeSource.width} × {voice.nativeSource.height} · {prefs.screenAudio ? 'avec le son' : 'sans le son'}</p>
            <label class="plbls-toggle">
              <input class="plbls-toggle__input" type="checkbox" bind:checked={prefs.screenAudio} onchange={() => prefs.save()} />
              <span class="plbls-toggle__piste"></span>
              <span class="plbls-toggle__texte">{voice.nativeSource.kind === 'window' ? 'Partager le son de cette fenêtre' : "Partager le son de l'ordinateur (sans la voix des participants)"}</span>
            </label>
          {:else}
            <div class="picker-preview">
              <video bind:this={shot} autoplay muted playsinline aria-label="Aperçu du partage"></video>
            </div>
            <p class="small muted">Aperçu de ce qui sera montré · {voice.preview?.getAudioTracks().length ? 'son partagé' : 'sans le son'}.</p>
          {/if}
        {/if}
        <div class="presets" role="radiogroup" aria-label="Qualité du partage">
          {#each PRESETS as p (p.id)}
            {@const on = eff.res === p.res && eff.fps === p.fps}
            <button class="preset" class:sel={on} role="radio" aria-checked={on} onclick={() => preset(p)}><strong>{p.label}</strong><span class="small muted">{p.hint}</span></button>
          {/each}
        </div>
        <details class="advanced">
          <summary>Avancé</summary>
        <div class="plbls-field">
          <label class="plbls-field__label" for="pq">Résolution</label>
          <select id="pq" class="plbls-select" value={eff.res} onchange={(e) => (prefs.screenRes = e.currentTarget.value as ScreenRes)}>
            {#each Object.keys(SCREEN_RES) as r (r)}<option value={r} disabled={!resOk(r as ScreenRes)}>{RES_LABEL[r as ScreenRes]}{resNote(r as ScreenRes)}</option>{/each}
          </select>
        </div>
        <div class="plbls-field">
          <label class="plbls-field__label" for="pf">Images par seconde</label>
          <select id="pf" class="plbls-select" value={eff.fps} onchange={(e) => (prefs.screenFps = Number(e.currentTarget.value) as ScreenFps)}>
            {#each SCREEN_FPS as f (f)}<option value={f} disabled={f > voice.limits.fps}>{f} i/s{f > voice.limits.fps ? ' (non proposé par le serveur)' : ''}</option>{/each}
          </select>
          <p class="small muted">Partage : <strong>{RES_LABEL[eff.res]} · {eff.fps} i/s</strong>, environ {mbps(screenBitrate(eff.res, eff.fps))} montants.
            {#if prefs.screenAuto}Si la connexion ou l'ordinateur ne suit pas, la qualité baisse toute seule (images/s d'abord, puis résolution).{:else}La qualité choisie est maintenue, même si la connexion est mauvaise.{/if}
            {#if eff.fps > 60}Beaucoup de systèmes plafonnent la capture d'écran à 60 i/s.{/if}
            {#if SCREEN_RES[eff.res].height > 1080}Au-delà de 1080p, chaque spectateur consomme aussi beaucoup côté serveur.{/if}</p>
        </div>
        </details>
        <label class="plbls-checkbox">
          <input class="plbls-checkbox__input" type="checkbox" bind:checked={prefs.screenAuto} />
          <span class="plbls-checkbox__case"></span>
          <span class="plbls-checkbox__texte">Réduire automatiquement si la connexion ne suit pas</span>
        </label>
      {:else}
        <CameraPreview oncams={(l) => (cams = l)} />
        <div class="plbls-field">
          <label class="plbls-field__label" for="pc">Caméra</label>
          <select id="pc" class="plbls-select" bind:value={prefs.cameraId}>
            <option value="">Par défaut</option>
            {#each cams as d, i (d.deviceId)}<option value={d.deviceId}>{d.label || `Caméra ${i + 1}`}</option>{/each}
          </select>
        </div>
        <CameraFxControls />
        <div class="plbls-field">
          <label class="plbls-field__label" for="pcq">Qualité</label>
          <select id="pcq" class="plbls-select" bind:value={prefs.cameraQuality}>
            {#each Object.keys(CAMERA) as q (q)}<option value={q}>{CAMERA_LABEL[q as CameraQuality]}</option>{/each}
          </select>
          <p class="small muted">Débit montant : environ {mbps(CAMERA[prefs.cameraQuality].bitrate)}.</p>
        </div>
      {/if}
      {#if !sourceStep}
      {#if voice.error}<p class="picker-err-line small" role="alert">{voice.error}</p>{/if}
      <div class="row picker-actions">
        <button class="plbls-button plbls-button--contour" onclick={close}>Annuler</button>
        {#if kind === 'screen' && !live}<button class="plbls-button plbls-button--contour" onclick={() => voice.changeSource()}>Changer de source</button>{/if}
        <button class="plbls-button" onclick={start} disabled={(kind === 'screen' && !live && !voice.preview && !voice.nativeSource)}>{live ? 'Appliquer' : kind === 'screen' ? 'Partager' : 'Activer la caméra'}</button>
      </div>
      {/if}
    </div>
  </Modal>
{/if}
