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

<li class="ptile" class:speaking class:screen={tile?.screen} class:big class:has-video={!!tile?.track} bind:this={el} ondblclick={fullscreen} use:ctx={menu}>
  {#if tile && !tile.track}
    <div class="ptile-ask">
      <div class="ptile-avatar"><Avatar pseudo={item.name} userId={item.userId} {serverId} /></div>
      <p class="ptile-ask-txt"><strong class="truncate">{item.name}</strong> <span class="ptile-tag"><Icon name="ecran" size={13} /> Écran</span></p>
      {#if part?.sound_off || part?.mic_off}<span class="ptile-mods">{@render mods()}</span>{/if}
      <button class="plbls-button plbls-button--compact v-rose" onclick={() => voice.watch(item.userId)}><Icon name="oeil" size={15} /> Regarder</button>
    </div>
  {:else if tile}
    <figure class="video-tile">
      <!-- svelte-ignore a11y_media_has_caption -->
      <video bind:this={video} use:attach={tile} autoplay playsinline muted class:mirror={self && !tile.screen} onresize={measure} onloadedmetadata={measure}></video>
      <figcaption class="ptile-meta">
        {#if speaking}{@render wave()}{/if}
        <Icon name={tile.screen ? 'ecran' : 'cam'} size={14} />
        <span class="truncate">{item.name}{self ? ' (vous)' : ''} · {tile.screen ? 'écran' : 'caméra'}{size ? ` · ${size}` : ''}</span>
        {@render states()}
        <Quality q={voice.quality[item.userId]} />
        <Watchers list={voice.watchers[tile.sid] ?? []} />
      </figcaption>
    </figure>
  {:else}
    <div class="ptile-avatar"><Avatar pseudo={item.name} userId={item.userId} {serverId} /></div>
    <div class="ptile-meta">
      {#if speaking}{@render wave()}{/if}
      <span class="truncate">{item.name}{self ? ' (vous)' : ''}</span>
      {@render states()}
      <Quality q={voice.quality[item.userId]} />
    </div>
  {/if}
  <div class="tile-tools">
    {#if adjustable}
      <button onclick={() => (vol = !vol)} aria-label="Volume de {item.name}" aria-expanded={vol} title="Volume de {item.name}"><Icon name="volmax" size={16} /></button>
    {/if}
    {#if tile?.track}
      {#if tile.screen && !self}
        <button class="stop-watch" onclick={() => voice.watch(item.userId, false)} title="Arrêter de regarder"><Icon name="oeil" size={16} /> Arrêter de regarder</button>
      {/if}
      <button onclick={onpin} aria-label={big ? 'Détacher' : 'Mettre en grand'} title={big ? 'Détacher' : 'Mettre en grand'}><Icon name={big ? 'reduire' : 'focus'} size={16} /></button>
      <button onclick={fullscreen} aria-label="Plein écran" title="Plein écran (double-clic)"><Icon name="agrandir" size={16} /></button>
    {/if}
  </div>
  {#if vol && adjustable}
    {@const screen = !!tile?.screen}
    {@const vols = screen ? prefs.screenVolumes : prefs.volumes}
    {@const muted = !!(screen ? prefs.mutedScreen : prefs.mutedVoice)[item.userId]}
    {@const label = screen ? 'Partage d\'écran' : 'Voix'}
    <div class="tile-vol">
      <button class:on={muted} onclick={() => voice.setUserMuted(item.userId, !muted, screen)} aria-pressed={muted} aria-label="Couper {label.toLowerCase()} de {item.name}" title="Couper {label.toLowerCase()}"><Icon name={muted ? 'casqueoff' : 'volmax'} size={16} /></button>
      <label><span class="tile-vol-lbl">{label} <span class="v-mono">{vols[item.userId] ?? 100} %</span></span>
        <input type="range" min="0" max="300" step="5" value={vols[item.userId] ?? 100} oninput={(e) => voice.setUserVolume(item.userId, +e.currentTarget.value, screen)} aria-label="Volume de {label.toLowerCase()} de {item.name}" />
      </label>
    </div>
  {/if}
</li>

{#snippet wave()}
  <span class="ptile-wave" aria-hidden="true"><i></i><i></i><i></i></span>
{/snippet}

{#snippet states()}
  {#if st?.deaf}<span class="ptile-st off" role="img" aria-label="Sourdine casque" title="Sourdine casque"><Icon name="casqueoff" size={14} /></span>{:else if st?.muted}<span class="ptile-st off" role="img" aria-label="Micro coupé" title="Micro coupé"><Icon name="micoff" size={14} /></span>{/if}
  {@render mods()}
{/snippet}

{#snippet mods()}
  {#if part?.sound_off || part?.mic_off}
    <span class="ptile-mod">
      <Icon name="bouclier" size={14} />
      {#if part?.sound_off}<span role="img" aria-label="Son coupé par un modérateur" title="Son coupé par un modérateur"><Icon name="casqueoff" size={14} /></span>{/if}
      {#if part?.mic_off}<span role="img" aria-label="Micro coupé par un modérateur" title="Micro coupé par un modérateur"><Icon name="micoff" size={14} /></span>{/if}
    </span>
  {/if}
{/snippet}

<style>
  .ptile { position: relative; min-width: 0; min-height: 0; border-radius: var(--r-l); background: var(--papier-3); color: var(--encre); overflow: hidden; display: grid; place-items: center; container-type: size; transition: box-shadow var(--d) var(--sortie); }
  .ptile.has-video { background: #0e0c0b; }
  .ptile.speaking { box-shadow: none; }
  .ptile.speaking::after { content: ''; position: absolute; inset: 0; z-index: 3; border-radius: inherit; box-shadow: inset 0 0 0 3px var(--rose-fonce); pointer-events: none; animation: ptile-respire 1.4s ease-in-out infinite; }
  @keyframes ptile-respire { 0%, 100% { box-shadow: inset 0 0 0 3px color-mix(in srgb, var(--rose-fonce) 60%, transparent); } 50% { box-shadow: inset 0 0 0 4px var(--rose-fonce); } }
  @media (prefers-reduced-motion: reduce) { .ptile.speaking::after { animation: none; } }
  :global(:root[data-motion='reduit']) .ptile.speaking::after { animation: none; }
  .ptile :global(.video-tile) { position: absolute; inset: 0; margin: 0; border-radius: 0; background: #0e0c0b; overflow: hidden; }
  .ptile :global(.video-tile video) { width: 100%; height: 100%; display: block; object-fit: cover; }
  .ptile :global(.video-tile video.mirror) { transform: scaleX(-1); }
  .ptile.screen :global(.video-tile video), .ptile.big :global(.video-tile video) { object-fit: contain; }
  .ptile-avatar { display: grid; place-items: center; }
  .ptile-avatar :global(.av) { width: clamp(32px, 30cqmin, 128px); height: clamp(32px, 30cqmin, 128px); margin: 0; border-radius: 30%; font-size: clamp(12px, 11cqmin, 46px); }
  .ptile-meta { position: absolute; left: 10px; bottom: 10px; z-index: 2; max-width: calc(100% - 20px); display: inline-flex; align-items: center; gap: 6px; height: 28px; margin: 0; padding: 0 10px; border-radius: var(--r-p); background: color-mix(in srgb, var(--surface) 88%, transparent); color: var(--encre); font-size: 13px; font-weight: 600; box-shadow: var(--o1); }
  .ptile-meta > :global(.icon) { flex: none; color: var(--gris); }
  .ptile-wave { display: inline-flex; align-items: center; gap: 2px; height: 12px; flex: none; color: var(--rose-fonce); }
  .ptile-wave i { display: block; width: 3px; border-radius: 2px; background: currentColor; animation: ptile-onde 0.9s ease-in-out infinite; }
  .ptile-wave i:nth-child(1) { height: 6px; }
  .ptile-wave i:nth-child(2) { height: 12px; animation-delay: -0.3s; }
  .ptile-wave i:nth-child(3) { height: 8px; animation-delay: -0.6s; }
  @keyframes ptile-onde { 50% { transform: scaleY(0.5); } }
  @media (prefers-reduced-motion: reduce) { .ptile-wave i { animation: none; } }
  .ptile-st, .ptile-mod, .ptile-mod > span { display: inline-flex; align-items: center; flex: none; }
  .ptile-st.off { color: var(--danger); }
  .ptile-mod { gap: 1px; color: var(--attention); }
  .ptile-ask { display: flex; flex-direction: column; align-items: center; gap: 8px; max-width: calc(100% - 24px); padding: 8px; text-align: center; }
  .ptile-ask .ptile-avatar :global(.av) { width: clamp(32px, 22cqmin, 72px); height: clamp(32px, 22cqmin, 72px); font-size: clamp(12px, 8cqmin, 26px); }
  .ptile-ask-txt { display: inline-flex; align-items: center; gap: 6px; max-width: 100%; margin: 0; font-size: 13px; }
  .ptile-tag { display: inline-flex; align-items: center; gap: 4px; height: 20px; padding: 0 7px; border-radius: var(--r-xs); background: var(--rose); color: var(--encre); font-size: 11px; font-weight: 600; flex: none; }
  .ptile-mods { display: inline-flex; }
  .tile-tools { position: absolute; top: 10px; right: 10px; z-index: 4; display: flex; gap: 4px; opacity: 0; transition: opacity var(--d) var(--sortie); }
  .ptile:hover .tile-tools, .ptile:focus-within .tile-tools { opacity: 1; }
  @media (hover: none) { .tile-tools { opacity: 1; } }
  .tile-tools button { display: inline-flex; align-items: center; justify-content: center; gap: 6px; min-width: 32px; height: 32px; padding: 0 8px; border: 0; border-radius: var(--r-s); background: var(--surface); color: var(--encre); box-shadow: var(--o1); font: inherit; font-size: 13px; font-weight: 600; cursor: pointer; transition: background var(--d) var(--sortie); }
  .tile-tools button:hover { background: var(--papier-2); }
  .tile-tools button:focus-visible, .tile-vol button:focus-visible { outline: none; box-shadow: var(--anneau); }
  .tile-vol { position: absolute; right: 10px; top: 50px; z-index: 5; display: flex; align-items: center; gap: 10px; padding: 10px 12px; border-radius: var(--r); background: var(--surface); color: var(--encre); box-shadow: var(--o2); font-size: 13px; }
  .tile-vol label { display: grid; gap: 4px; }
  .tile-vol-lbl { display: flex; justify-content: space-between; gap: 10px; font-weight: 600; }
  .tile-vol input { accent-color: var(--rose-fonce); }
  .tile-vol button { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; border: 0; border-radius: var(--r-s); background: var(--papier-2); color: var(--encre); cursor: pointer; }
  .tile-vol button.on { background: var(--danger-fond); color: var(--danger); }
  @media (pointer: coarse) { .tile-tools button, .tile-vol button { min-width: 44px; height: 44px; } }
</style>
