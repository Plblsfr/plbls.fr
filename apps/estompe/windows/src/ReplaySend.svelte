<script lang="ts">
  import { app } from '../../../../web/estompe/src/lib/store.svelte'
  import { lastDestination, sendDestinations, sends } from './replaySend.svelte'
  import { compressedSize, filterDestinations, formatDuration, formatSize, needsCompression, targetKbps, type Replay } from './replays'
  import Modal from '../../../../web/estompe/src/components/Modal.svelte'

  let { replay, dir, onclose }: { replay: Replay; dir: string; onclose: () => void } = $props()

  const all = $derived(sendDestinations())
  let query = $state('')
  let message = $state('')
  let choice = $state<string | null>(null)
  let wantCompress = $state(false)
  const shown = $derived(filterDestinations(all, query))
  const groups = $derived([...new Set(shown.map((d) => d.group))])
  const to = $derived(choice ?? lastDestination()?.key ?? null)
  const target = $derived(all.find((d) => d.key === to))

  const tooBig = $derived(needsCompression(replay.size, app.maxVideo))
  const compress = $derived(tooBig || wantCompress)
  const kbps = $derived(targetKbps(app.maxVideo, replay.duration))
  const estimate = $derived(compress ? compressedSize(replay, kbps) : replay.size)

  function submit(e: Event) {
    e.preventDefault()
    if (!target) return
    sends.start({ replay, dir, to: target.key, label: target.label, message: message.trim(), compress })
    onclose()
  }
</script>

<Modal title="Envoyer dans…" {onclose}>
  <form class="stack" onsubmit={submit}>
    <p class="small"><strong class="truncate">{replay.name}</strong><br /><span class="muted">{formatDuration(replay.duration)} · {replay.width}×{replay.height} · {formatSize(replay.size)}</span></p>
    <input class="plbls-input" type="search" placeholder="Rechercher un salon ou une personne" aria-label="Rechercher une destination" bind:value={query} />
    <div class="rs-list" role="radiogroup" aria-label="Destination">
      {#each groups as g (g)}
        <div class="rs-group muted small">{g}</div>
        {#each shown.filter((d) => d.group === g) as d (d.key)}
          <button type="button" class="rs-item" role="radio" aria-checked={to === d.key} class:on={to === d.key} onclick={() => (choice = d.key)}>{d.label}</button>
        {/each}
      {/each}
      {#if !shown.length}<p class="muted small">Aucune destination.</p>{/if}
    </div>
    <div class="plbls-field">
      <label class="plbls-field__label" for="rs-msg">Message (facultatif)</label>
      <textarea id="rs-msg" class="plbls-input" rows="2" maxlength="4000" placeholder="Markdown et @pseudo comme dans un message" bind:value={message}></textarea>
    </div>
    {#if tooBig}
      <div class="plbls-alert plbls-alert--info" role="status"><p class="plbls-alert__corps">Ce replay dépasse la limite de {formatSize(app.maxVideo)} : il sera envoyé en 720p, ≈ {formatSize(estimate)}.</p></div>
    {:else}
      <label class="plbls-checkbox"><input class="plbls-checkbox__input" type="checkbox" bind:checked={wantCompress} /><span class="plbls-checkbox__case" aria-hidden="true"></span><span class="plbls-checkbox__texte">Envoyer une version compressée (720p, ≈ {formatSize(compressedSize(replay, kbps))})</span></label>
    {/if}
    <button class="plbls-button" disabled={!target}>Envoyer{target ? ` dans ${target.label}` : ''} (≈ {formatSize(estimate)})</button>
  </form>
</Modal>

<style>
  .rs-list { max-height: 240px; overflow: auto; display: flex; flex-direction: column; gap: 2px; border: 1px solid var(--bordure, #8884); border-radius: 8px; padding: 4px; }
  .rs-group { padding: 6px 6px 2px; }
  .rs-item { text-align: left; border: 0; background: transparent; color: inherit; padding: 6px 10px; border-radius: 6px; cursor: pointer; }
  .rs-item:hover { background: #8882; }
  .rs-item.on { background: color-mix(in srgb, var(--accent, #4a8) 30%, transparent); }
</style>
