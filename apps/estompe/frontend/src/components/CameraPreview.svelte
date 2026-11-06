<script lang="ts">
  import type { LocalVideoTrack } from 'livekit-client'
  import { applyCameraFx } from '../lib/camerafx'
  import { prefs } from '../lib/prefs.svelte'

  let { oncams }: { oncams?: (list: MediaDeviceInfo[]) => void } = $props()
  let video = $state<HTMLVideoElement>()
  let track = $state<LocalVideoTrack>()
  let error = $state('')

  $effect(() => {
    if (!video) return
    let stream: MediaStream | undefined
    let lk: LocalVideoTrack | undefined
    let dead = false
    error = ''
    navigator.mediaDevices
      .getUserMedia({ video: { deviceId: prefs.cameraId ? { exact: prefs.cameraId } : undefined, width: { ideal: 1280 }, height: { ideal: 720 } } })
      .then(async (s) => {
        if (dead) return s.getTracks().forEach((t) => t.stop())
        stream = s
        const { LocalVideoTrack } = await import('livekit-client')
        lk = new LocalVideoTrack(s.getVideoTracks()[0])
        track = lk
        oncams?.((await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === 'videoinput')) // étiquettes lisibles une fois l'accès accordé
      })
      .catch(() => (error = 'Caméra indisponible ou accès refusé.'))
    return () => { dead = true; track = undefined; lk?.stop(); stream?.getTracks().forEach((t) => t.stop()); if (video) video.srcObject = null }
  })

  // L'effet suit les réglages en direct ; « Aucun » n'ouvre aucun modèle.
  $effect(() => {
    const t = track
    void [prefs.cameraFx, prefs.cameraBlur, prefs.cameraBg, prefs.customBg]
    if (!t || !video) return
    void applyCameraFx(t, () => { prefs.cameraFx = 'none'; prefs.save() }).then((ok) => {
      if (!ok) error = "Effet d'arrière-plan indisponible sur cet appareil."
      if (video && track === t) video.srcObject = new MediaStream([t.mediaStreamTrack])
    })
  })
</script>

<div class="cam-prev">
  <video bind:this={video} autoplay muted playsinline aria-label="Aperçu de la caméra (en miroir)"></video>
  {#if error}<p class="picker-err small">{error}</p>{/if}
</div>

<style>
  /* Hauteur réservée par le rembourrage (et non aspect-ratio) : l'aperçu ne recouvre pas les réglages dans la grille des Paramètres. */
  .cam-prev { position: relative; padding-top: 56.25%; background: #000; border-radius: var(--rayon); overflow: hidden; }
  video { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; transform: scaleX(-1); }
</style>
