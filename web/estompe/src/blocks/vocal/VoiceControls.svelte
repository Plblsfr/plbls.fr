<script lang="ts">
  import { voice, SHORTCUT } from './voice.svelte'
  import { app } from '../../lib/store.svelte'
  import { prefs } from '../../lib/prefs.svelte'
  import { platform } from '../../lib/platform'
  import { menuFrom, type MenuItem } from '../../lib/ctxmenu.svelte'
  import { openSettings } from '../../lib/settings.svelte'
  import Icon from '../../components/Icon.svelte'
  import Quality from '../../components/Quality.svelte'

  // Les mêmes commandes servent dans le panneau latéral (`panel` : qualité dans l'en-tête, options dans un menu « … ») et dans la barre flottante du mode plein cadre.
  let { panel = false }: { panel?: boolean } = $props()

  const where = $derived(voice.channelId ? app.channel(voice.channelId) : null)
  const allowed = (p: 'speak' | 'video' | 'share_screen') => !where || app.can(where.server.id, p, where.channel.id)
  const canShare = typeof navigator !== 'undefined' && !!navigator.mediaDevices?.getDisplayMedia && allowed('share_screen')
  const pct = (db: number) => Math.min(100, Math.max(0, ((db + 80) / 80) * 100)) // jauge de -80 à 0 dBFS
  const mic = $derived(voice.muted ? 'Réactiver le micro' : 'Couper le micro')
  const ear = $derived(voice.deafened ? 'Réactiver le son' : 'Couper le son')
  const cam = $derived(voice.camera ? 'Couper la caméra' : 'Activer la caméra')
  const tip = (label: string, key?: string) => (key ? `${label} (${key})` : label)
  const me = $derived(voice.quality[app.meId])
  // parle alors que le micro est coupé : le signal du micro dépasse le seuil de voix
  const mutedTalking = $derived(voice.muted && !voice.deafened && voice.level > prefs.vadThreshold)

  // Choix rapide du micro ou de la sortie audio : la liste n'est lisible qu'une fois l'accès au micro accordé (appel en cours).
  async function devices(e: MouseEvent, kind: 'audioinput' | 'audiooutput') {
    const from = e.currentTarget as Element
    const list = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === kind)
    const cur = kind === 'audioinput' ? prefs.micId : prefs.speakerId
    const items: MenuItem[] = [{ label: 'Par défaut', checked: !cur, run: () => choose(kind, '') }]
    list.filter((d) => d.deviceId !== 'default').forEach((d, i) => items.push({ label: d.label || (kind === 'audioinput' ? `Micro ${i + 1}` : `Sortie ${i + 1}`), checked: d.deviceId === cur, run: () => choose(kind, d.deviceId) }))
    items.push({ label: kind === 'audioinput' ? 'Seuil de voix et réglages audio…' : 'Réglages audio…', sep: true, run: () => openSettings('voice') })
    menuFrom(from, items, true)
  }
  function choose(kind: 'audioinput' | 'audiooutput', id: string) {
    if (kind === 'audioinput') prefs.micId = id
    else prefs.speakerId = id
    prefs.save()
    void voice.switchDevice(kind)
  }
  const overlay = platform.voiceOverlay
  const ov = $derived(overlay?.hidden() ? "Afficher l'overlay des voix" : "Masquer l'overlay des voix")
  const scr = $derived(voice.screen ? 'Arrêter le partage' : "Partager l'écran")
  const showOverlay = !!overlay?.available()
  // Dans le panneau (largeur fixe), le partage puis la caméra passent dans le menu « … » quand la rangée est trop étroite.
  let w = $state(0)
  const showShare = $derived(!panel || !w || w >= 264)
  const showCam = $derived(!panel || !w || w >= 224)
  const silent = $derived(prefs.vad && !prefs.ptt && !voice.muted && !voice.voiceActive)

  function more(e: MouseEvent) {
    const items: MenuItem[] = []
    if (!showCam && allowed('video')) items.push({ label: cam, run: () => voice.requestCamera(), checked: voice.camera })
    if (!showShare && canShare) items.push({ label: scr, run: () => voice.requestScreen(), checked: voice.screen })
    if (canShare && voice.screen) items.push({ label: 'Qualité du partage', run: () => (voice.picker = 'screen') })
    if (showOverlay) items.push({ label: ov, run: overlay!.toggle, checked: overlay!.hidden() })
    menuFrom(e.currentTarget as Element, items, true)
  }
</script>

<div class="vctl" bind:clientWidth={w} role="group" aria-label="Commandes vocales">
  {#if !panel}<span class="vquality"><Quality q={me} /></span><span class="vsep" aria-hidden="true"></span>{/if}
  {#if allowed('speak')}
    <span class="vsplit">
      <button class="vbtn" class:off={voice.muted} class:silent aria-pressed={voice.muted} onclick={() => voice.toggleMute()} aria-label={mic} title={tip(mic, SHORTCUT.mute)}><Icon name={voice.muted ? 'mic-off' : 'mic'} size={20} />{#if !voice.muted}<span class="vlevel" aria-hidden="true" style:width="{pct(voice.level)}%" class:over={voice.level > prefs.vadThreshold}></span>{/if}</button>
      <button class="vcaret" onclick={(e) => void devices(e, 'audioinput')} aria-haspopup="menu" aria-label="Choisir le micro" title="Choisir le micro"><Icon name="chevron" size={12} /></button>
    </span>
  {/if}
  <span class="vsplit">
    <button class="vbtn" class:off={voice.deafened} aria-pressed={voice.deafened} onclick={() => voice.toggleDeafen()} aria-label={ear} title={tip(ear, SHORTCUT.deafen)}><Icon name={voice.deafened ? 'headphones-off' : 'headphones'} size={20} /></button>
    <button class="vcaret" onclick={(e) => void devices(e, 'audiooutput')} aria-haspopup="menu" aria-label="Choisir la sortie audio" title="Choisir la sortie audio"><Icon name="chevron" size={12} /></button>
  </span>
  {#if allowed('video') && showCam}<button class="vbtn" class:on={voice.camera} aria-pressed={voice.camera} onclick={() => voice.requestCamera()} aria-label={cam} title={tip(cam, SHORTCUT.camera)}><Icon name={voice.camera ? 'camera' : 'camera-off'} size={20} /></button>{/if}
  {#if canShare && showShare}
    <button class="vbtn" class:on={voice.screen} aria-pressed={voice.screen} onclick={() => voice.requestScreen()} aria-label={scr} title={tip(scr, SHORTCUT.screen)}><Icon name="monitor" size={20} /></button>
  {/if}
  {#if panel}
    {#if canShare && voice.screen || showOverlay || !showShare || !showCam}
      <button class="vbtn" onclick={more} aria-haspopup="menu" aria-label="Plus d'options" title="Plus d'options"><Icon name="more" size={20} /></button>
    {/if}
  {:else}
    {#if canShare && voice.screen}
      <button class="vbtn" onclick={() => (voice.picker = 'screen')} aria-label="Qualité du partage" title="Qualité du partage"><Icon name="sliders" size={20} /></button>
    {/if}
    {#if showOverlay}
      <button class="vbtn" class:off={overlay!.hidden()} aria-pressed={overlay!.hidden()} onclick={overlay!.toggle} aria-label={ov} title={ov}><Icon name="eye" size={20} /></button>
    {/if}
  {/if}
  <span class="vsep vend" aria-hidden="true"></span>
  <button class="vbtn leave" onclick={() => voice.leave()} aria-label="Quitter le vocal" title="Quitter le vocal"><Icon name="phone-off" size={20} /></button>
</div>

{#if mutedTalking}<p class="small vwarn" role="status">Ton micro est coupé</p>{/if}

{#if panel && prefs.ptt}
  <p class="small muted vptt">Push-to-talk : maintiens <kbd>{prefs.pttKey.replace(/^Key/, '')}</kbd>{voice.talking ? ' · tu parles' : ''}</p>
{/if}
