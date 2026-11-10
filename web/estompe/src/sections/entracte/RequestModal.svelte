<script lang="ts">
  import Modal from '../../components/Modal.svelte'
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

<Modal title="Demander « {title} »" {onclose}>
  <form class="req" onsubmit={(e) => { e.preventDefault(); void send() }}>
    {#if kind === 'movie'}
      <p class="v-note">Ce film n’est pas sur le serveur. L’administrateur verra ta demande et pourra l’accepter ; tu seras prévenu quand il sera disponible.</p>
    {:else if !series && !error}
      <Loader />
    {:else if series}
      {#if open.length}
        <label class="plbls-toggle req-all">
          <input type="checkbox" class="plbls-toggle__input" checked={all} onchange={toggleAll} />
          <span class="plbls-toggle__piste"></span>
          <span class="plbls-toggle__texte"><b>Toutes les saisons manquantes</b></span>
        </label>
      {/if}
      <ul class="req-seasons">
        {#each series.seasons as s (s.number)}
          {@const blocked = s.complete || !!s.request_state}
          <li>
            <label class="req-season" class:off={blocked}>
              <input type="checkbox" disabled={blocked} checked={chosen.includes(s.number)} onchange={() => toggle(s.number)} />
              <span class="grow"><strong>{s.title}</strong> <span class="v-note">{s.available_count}/{s.episode_count} épisodes</span></span>
              {#if s.complete}<span class="v-tag ok">Disponible</span>{:else if s.request_state}<span class="v-tag">{stateLabel(s.request_state)}</span>{/if}
            </label>
          </li>
        {/each}
      </ul>
      {#if !open.length}<p class="v-note">Toutes les saisons sont déjà là ou déjà demandées.</p>{/if}
    {/if}
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    <div class="req-actions">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button type="submit" class="plbls-button" disabled={busy || (kind === 'series' && !chosen.length)}><Icon name="envoi" size={16} /> Demander</button>
    </div>
  </form>
</Modal>

<style>
  .req { display: flex; flex-direction: column; gap: var(--espace-3); }
  .req-seasons { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-1); }
  .req-season { display: flex; align-items: center; gap: var(--espace-2); padding: var(--espace-2); border-radius: var(--r); background: var(--surface); }
  .req-season.off { opacity: 0.6; }
  .req-actions { display: flex; justify-content: flex-end; gap: var(--espace-2); }
</style>
