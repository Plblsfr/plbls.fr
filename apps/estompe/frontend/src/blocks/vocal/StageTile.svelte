<script lang="ts">
  import type { StageItem } from './stage'
  import { voice } from './voice.svelte'
  import { prefs } from '../../lib/prefs.svelte'
  import { app } from '../../lib/store.svelte'
  import { ctx } from '../../lib/ctxmenu.svelte'
  import { participantMenu } from '../../lib/volume'
  import Avatar from '../../components/Avatar.svelte'
  import Icon from '../../components/Icon.svelte'
  import Quality from '../../components/Quality.svelte'
  import Watchers from '../regarder-ensemble/Watchers.svelte'
  import { outranks } from '../../lib/roles'

  let { item, self, serverId = '', big = false, onpin }: { item: StageItem; self: boolean; serverId?: string; big?: boolean; onpin: () => void } = $props()

  const tile = $derived(item.tile)
  const st = $derived(voice.states[item.userId])
  // pas de bordure sur la tuile d'écran, ni pour un micro coupé (le son d'un écran partagé fait aussi « parler » LiveKit)
  const speaking = $derived(!!voice.speaking[item.userId] && !tile?.screen && !st?.muted && !st?.deaf)
  let video = $state<HTMLVideoElement>()
  let el = $state<HTMLElement>()
  let size = $state('')
  let vol = $state(false)
  // le son d'un partage n'est réglable que s'il en a un
  const adjustable = $derived(!self && (!tile?.screen || !!voice.screenAudio[item.userId]))
  // coupures du serveur (EST-102), visibles de tous ; le créateur peut les poser sur les autres
  const part = $derived(app.voice[voice.channelId ?? '']?.find((p) => p.user_id === item.userId))
  const channelId = $derived(voice.channelId ?? '')
  const menu = () => participantMenu({
    self, muted: !!prefs.mutedVoice[item.userId], canModerate: outranks(app.details[serverId], app.meId, item.userId), canMute: app.can(serverId, 'mute_members', channelId), canDeafen: app.can(serverId, 'deafen_members', channelId), mod: part ?? {},
    setMuted: (m) => voice.setUserMuted(item.userId, m), adjust: adjustable ? () => (vol = true) : undefined,
    moderate: (m) => void voice.moderate(voice.channelId ?? '', item.userId, m),
  })

  // La tuile affichée chez nous compte comme un spectateur du flux (pas pour notre propre flux).
  $effect(() => {
    const sid = tile?.track && tile.sid
    if (sid && !self) return voice.view(sid)
  })

  function attach(node: HTMLVideoElement, t: NonNullable<StageItem['tile']>) {
    let cur = t
    cur.track?.attach(node)
    return {
      update(next: typeof t) {
        if (next.track === cur.track) return
        cur.track?.detach(node)
        cur = next
        cur.track?.attach(node)
      },
      destroy() { cur.track?.detach(node) },
    }
  }

  // Résolution réellement reçue (varie avec la connexion et la taille de la tuile) : utile pour juger la qualité.
  const measure = () => { size = video && video.videoHeight ? `${video.videoHeight}p` : '' }

  function fullscreen() {
    if (document.fullscreenElement) void document.exitFullscreen()
    else void el?.requestFullscreen?.()
  }
</script>

<li class="ptile" class:speaking class:screen={tile?.screen} class:big bind:this={el} ondblclick={fullscreen} use:ctx={menu}>
  {#if tile && !tile.track}
    <div class="ptile-avatar"><Avatar pseudo={item.name} userId={item.userId} {serverId} /></div>
    <div class="ptile-meta">
      <span class="truncate">{item.name}</span>
      <span class="live"><Icon name="monitor" size={12} /> Écran</span>
      {@render mods()}
      <button class="plbls-button plbls-button--compact" onclick={() => voice.watch(item.userId)}>Regarder</button>
    </div>
  {:else if tile}
    <figure class="video-tile">
      <!-- svelte-ignore a11y_media_has_caption -->
      <video bind:this={video} use:attach={tile} autoplay playsinline muted class:mirror={self && !tile.screen} onresize={measure} onloadedmetadata={measure}></video>
      <figcaption class="ptile-meta">
        <span class="truncate">{item.name}{self ? ' (vous)' : ''} · {tile.screen ? 'écran' : 'caméra'}{size ? ` · ${size}` : ''}</span>
        {#if st?.deaf}<span role="img" aria-label="Sourdine casque"><Icon name="headphones-off" size={14} /></span>{:else if st?.muted}<span role="img" aria-label="Micro coupé"><Icon name="mic-off" size={14} /></span>{/if}
        {@render mods()}
        <Quality q={voice.quality[item.userId]} />
        <Watchers list={voice.watchers[tile.sid] ?? []} />
      </figcaption>
    </figure>
  {:else}
    <div class="ptile-avatar"><Avatar pseudo={item.name} userId={item.userId} {serverId} {speaking} /></div>
    <div class="ptile-meta">
      <span class="truncate">{item.name}{self ? ' (vous)' : ''}</span>
      {#if st?.deaf}<span role="img" aria-label="Sourdine casque"><Icon name="headphones-off" size={14} /></span>{:else if st?.muted}<span role="img" aria-label="Micro coupé"><Icon name="mic-off" size={14} /></span>{/if}
      {@render mods()}
      <Quality q={voice.quality[item.userId]} />
    </div>
  {/if}
  <div class="tile-tools">
    {#if adjustable}
      <button onclick={() => (vol = !vol)} aria-label="Volume de {item.name}" aria-expanded={vol} title="Volume de {item.name}"><Icon name="volume" size={16} /></button>
    {/if}
    {#if tile?.track}
      {#if tile.screen && !self}
        <button class="stop-watch" onclick={() => voice.watch(item.userId, false)} title="Arrêter de regarder"><Icon name="eye" size={16} /> Arrêter de regarder</button>
      {/if}
      <button onclick={onpin} aria-label={big ? 'Détacher' : 'Mettre en grand'} title={big ? 'Détacher' : 'Mettre en grand'}><Icon name={big ? 'minimize' : 'focus'} size={16} /></button>
      <button onclick={fullscreen} aria-label="Plein écran" title="Plein écran (double-clic)"><Icon name="expand" size={16} /></button>
    {/if}
  </div>
  {#if vol && adjustable}
    {@const screen = !!tile?.screen}
    {@const vols = screen ? prefs.screenVolumes : prefs.volumes}
    {@const muted = !!(screen ? prefs.mutedScreen : prefs.mutedVoice)[item.userId]}
    {@const label = screen ? 'Partage d\'écran' : 'Voix'}
    <div class="tile-vol small">
      <button onclick={() => voice.setUserMuted(item.userId, !muted, screen)} aria-pressed={muted} aria-label="Couper {label.toLowerCase()} de {item.name}" title="Couper {label.toLowerCase()}"><Icon name={muted ? 'headphones-off' : 'volume'} size={14} /></button>
      <label>{label} {vols[item.userId] ?? 100} %
        <input type="range" min="0" max="300" step="5" value={vols[item.userId] ?? 100} oninput={(e) => voice.setUserVolume(item.userId, +e.currentTarget.value, screen)} aria-label="Volume de {label.toLowerCase()} de {item.name}" />
      </label>
    </div>
  {/if}
</li>

{#snippet mods()}
  {#if part?.sound_off}<span class="mod" role="img" aria-label="Son coupé par un modérateur" title="Son coupé par un modérateur"><Icon name="headphones-off" size={14} /></span>{/if}
  {#if part?.mic_off}<span class="mod" role="img" aria-label="Micro coupé par un modérateur" title="Micro coupé par un modérateur"><Icon name="mic-off" size={14} /></span>{/if}
{/snippet}
