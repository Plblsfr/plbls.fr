<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from '../lib/api'
  import { BACKGROUNDS, FX_LABEL, prepareBackground } from '../lib/camerafx'
  import { prefs, type CameraFx } from '../lib/prefs.svelte'
  import { voice } from '../blocks/vocal/voice.svelte'

  let error = $state('')
  const changed = () => { prefs.save(); void voice.applyFx() }
  const toDataUrl = (b: Blob) => new Promise<string>((ok, ko) => { const r = new FileReader(); r.onload = () => ok(r.result as string); r.onerror = ko; r.readAsDataURL(b) })

  // L'image perso enregistrée sur le serveur est retrouvée sur un nouvel appareil.
  onMount(() => {
    if (!prefs.customBg) api.cameraBackground().then(toDataUrl).then((u) => { prefs.customBg = u; prefs.save() }).catch(() => {})
  })

  async function upload(e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    error = ''
    try {
      const img = await prepareBackground(file)
      await api.setCameraBackground(img)
      prefs.customBg = await toDataUrl(img)
      prefs.cameraBg = 'custom'
      prefs.cameraFx = 'image'
      changed()
    } catch (err) {
      error = err instanceof Error ? err.message : 'Image refusée.'
    }
  }
  async function removeCustom() {
    await api.deleteCameraBackground().catch(() => {})
    prefs.customBg = ''
    if (prefs.cameraBg === 'custom') prefs.cameraBg = BACKGROUNDS[0].id
    changed()
  }
</script>

<div class="plbls-field">
  <label class="plbls-field__label" for="cfx">Arrière-plan de la caméra</label>
  <select id="cfx" class="plbls-select" bind:value={prefs.cameraFx} onchange={changed}>
    {#each Object.keys(FX_LABEL) as f (f)}<option value={f as CameraFx}>{FX_LABEL[f as CameraFx]}</option>{/each}
  </select>
  {#if prefs.cameraFx !== 'none'}<p class="small muted">Traité sur votre appareil, sans envoi d'image à un serveur. Les autres personnes le voient sans rien régler.</p>{/if}
</div>
{#if prefs.cameraFx === 'blur'}
  <div class="plbls-field">
    <label class="plbls-field__label" for="cblur">Intensité du flou : {prefs.cameraBlur}</label>
    <input id="cblur" type="range" min="2" max="30" step="1" bind:value={prefs.cameraBlur} onchange={changed} />
  </div>
{/if}
{#if prefs.cameraFx === 'image'}
  <div class="plbls-field">
    <label class="plbls-field__label" for="cbg">Fond</label>
    <select id="cbg" class="plbls-select" bind:value={prefs.cameraBg} onchange={changed}>
      {#each BACKGROUNDS as b (b.id)}<option value={b.id}>{b.label}</option>{/each}
      {#if prefs.customBg}<option value="custom">Mon image</option>{/if}
    </select>
    <input type="file" accept="image/png,image/jpeg,image/webp" aria-label="Envoyer une image de fond" onchange={upload} />
    {#if prefs.customBg}<button class="plbls-button plbls-button--contour" onclick={removeCustom}>Supprimer mon image</button>{/if}
    {#if error}<p class="small" role="alert">{error}</p>{/if}
  </div>
{/if}
