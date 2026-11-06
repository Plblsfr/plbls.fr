<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { formatDuration } from '../../lib/format'

  let { src, poster, autoplay = false, onerror }: { src: string; poster?: string; autoplay?: boolean; onerror?: () => void } = $props()

  let box: HTMLDivElement
  let video: HTMLVideoElement
  let paused = $state(true)
  let time = $state(0)
  let duration = $state(0)
  let volume = $state(1)
  let muted = $state(false)
  let seeking = false

  const toggle = () => (video.paused ? void video.play().catch(() => {}) : video.pause())
  const fullscreen = () => (document.fullscreenElement ? document.exitFullscreen() : box.requestFullscreen?.())
  function key(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement) return
    if (e.key === ' ' || e.key === 'k') toggle()
    else if (e.key === 'ArrowLeft') video.currentTime = Math.max(0, video.currentTime - 5)
    else if (e.key === 'ArrowRight') video.currentTime = Math.min(duration, video.currentTime + 5)
    else if (e.key === 'm') muted = !muted
    else if (e.key === 'f') void fullscreen()
    else return
    e.preventDefault()
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="vp" bind:this={box} role="group" aria-label="Lecteur vidéo" tabindex="-1" onkeydown={key}>
  <!-- svelte-ignore a11y_media_has_caption -->
  <video bind:this={video} {src} {poster} {autoplay} preload="metadata" playsinline bind:paused bind:volume bind:muted bind:duration
    ontimeupdate={() => { if (!seeking) time = video.currentTime }} onclick={toggle} {onerror}></video>
  <div class="vp-bar">
    <button class="ib" onclick={toggle} aria-label={paused ? 'Lire' : 'Pause'}><Icon name={paused ? 'play' : 'pause'} size={16} /></button>
    <span class="vp-time small">{formatDuration(time)} / {formatDuration(duration || 0)}</span>
    <input class="vp-seek" type="range" min="0" max={duration || 0} step="0.1" value={time} aria-label="Position" disabled={!duration}
      oninput={(e) => { seeking = true; time = +e.currentTarget.value }} onchange={() => { video.currentTime = time; seeking = false }} />
    <button class="ib" class:vp-off={muted || volume === 0} onclick={() => (muted = !muted)} aria-label={muted ? 'Activer le son' : 'Couper le son'}><Icon name="volume" size={16} /></button>
    <input class="vp-vol" type="range" min="0" max="1" step="0.05" bind:value={volume} aria-label="Volume" />
    <button class="ib" onclick={fullscreen} aria-label="Plein écran"><Icon name="expand" size={16} /></button>
  </div>
</div>
