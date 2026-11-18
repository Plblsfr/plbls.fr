<script lang="ts">
  import Loader from '../../components/Loader.svelte'
  import Icon from '../../components/Icon.svelte'
  import { entracteApi } from '../../lib/api'
  import { messageOf, notify } from '../../lib/ui'
  import { requestableSeasons, requestBody, stateLabel, type EntracteRequest, type RequestKind, type RequestableSeries } from './entracte'

  let { kind, tmdbId, title, onclose, ondone }: { kind: RequestKind; tmdbId: number; title: string; onclose: () => void; ondone: (r: EntracteRequest) => void } = $props()

  let series = $state<RequestableSeries | null>(null)
  let chosen = $state<number[]>([])
  let busy = $state(false)
  let error = $state('')

  const open = $derived(series ? requestableSeasons(series) : [])
  const all = $derived(open.length > 0 && open.every((s) => chosen.includes(s.number)))

  $effect(() => {
    if (kind !== 'series') return
    let gone = false
    void entracteApi.series(tmdbId).then((s) => {
      if (gone) return
      series = s
      chosen = requestableSeasons(s).map((x) => x.number)
    }).catch((e) => { if (!gone) error = messageOf(e, 'Les saisons de cette série sont indisponibles.') })
    return () => { gone = true }
  })

  const toggleAll = () => { chosen = all ? [] : open.map((s) => s.number) }
  const toggle = (n: number) => { chosen = chosen.includes(n) ? chosen.filter((x) => x !== n) : [...chosen, n] }

  async function send() {
    busy = true
    error = ''
    try {
      const r = await entracteApi.request(requestBody(kind, tmdbId, chosen, series ?? undefined))
      notify.ok(r.joined_existing ? 'Ce titre était déjà demandé : tu seras prévenu toi aussi.' : 'Demande envoyée à l’administrateur.')
      ondone(r)
      onclose()
    } catch (e) {
      error = messageOf(e, 'La demande n’a pas pu être envoyée.')
    } finally {
      busy = false
    }
  }
</script>

<form class="req" aria-label={`Demander ${title}`} onsubmit={(e) => { e.preventDefault(); void send() }}>
  {#if kind === 'movie'}
    <p class="v-note">Ce film n’est pas sur le serveur. L’administrateur verra ta demande ; tu seras prévenu dès qu’il est disponible, ou si elle est refusée.</p>
  {:else if !series && !error}
    <Loader />
  {:else if series}
    <fieldset class="req-saisons">
      <legend>Que veux-tu demander ?</legend>
      {#if open.length > 1}
        <label class="req-puce" class:on={all}><input type="checkbox" checked={all} onchange={toggleAll} /> Toutes les saisons manquantes</label>
      {/if}
      {#each series.seasons as s (s.number)}
        {@const blocked = s.complete || !!s.request_state}
        <label class="req-puce" class:on={chosen.includes(s.number)} class:off={blocked}>
          <input type="checkbox" disabled={blocked} checked={chosen.includes(s.number)} onchange={() => toggle(s.number)} />
          <span>{s.title} <span class="v-note">{s.complete ? 'disponible' : s.request_state ? stateLabel(s.request_state).toLowerCase() : `${s.available_count}/${s.episode_count}`}</span></span>
        </label>
      {/each}
    </fieldset>
    {#if !open.length}<p class="v-note">Toutes les saisons sont déjà là ou déjà demandées.</p>{/if}
  {/if}
  {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
  <div class="req-actions">
    <button type="submit" class="plbls-button" disabled={busy || (kind === 'series' && !chosen.length)}><Icon name="envoi" size={16} /> Envoyer la demande</button>
    <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
  </div>
</form>

<style>
  .req { display: flex; flex-direction: column; gap: var(--espace-3); }
  .req > p { margin: 0; }
  .req-saisons { display: flex; flex-wrap: wrap; gap: var(--espace-2); margin: 0; padding: 0; border: 0; }
  .req-saisons legend { width: 100%; padding: 0 0 var(--espace-2); font-weight: 600; }
  .req-puce { display: inline-flex; align-items: center; gap: var(--espace-2); min-height: 44px; padding: 0 var(--espace-4); border-radius: var(--r-p); background: var(--papier-2); cursor: pointer; }
  .req-puce.on { background: var(--rose); font-weight: 600; }
  .req-puce.off { opacity: 0.6; cursor: default; }
  .req-puce input { width: 18px; height: 18px; accent-color: var(--encre); }
  .req-puce:has(input:focus-visible) { box-shadow: var(--anneau); }
  .req-actions { display: flex; flex-wrap: wrap; gap: var(--espace-2); }
</style>
