<script lang="ts">
  import Modal from '../../components/Modal.svelte'
  import Loader from '../../components/Loader.svelte'
  import Icon from '../../components/Icon.svelte'
  import { entracteApi } from '../../lib/api'
  import { messageOf } from '../../lib/ui'
  import { freeSpace, initialChoice, type EntracteRequest, type SendChoice, type SendOptions } from './entracte'

  let { request, onclose, ondone }: { request: EntracteRequest; onclose: () => void; ondone: (r: EntracteRequest) => void } = $props()

  let options = $state<SendOptions | null>(null)
  let choice = $state<SendChoice | null>(null)
  let unavailable = $state('')
  let error = $state('')
  let busy = $state(false)

  const service = $derived(request.kind === 'movie' ? 'Radarr' : 'Sonarr')

  $effect(() => {
    let gone = false
    void entracteApi.sendOptions(request.kind).then((o) => {
      if (gone) return
      options = o
      choice = initialChoice(o)
      if (!choice) unavailable = `${service} ne déclare aucun emplacement ou aucun profil de qualité.`
    }).catch((e) => { if (!gone) unavailable = messageOf(e, `Les réglages de ${service} sont indisponibles.`) })
    return () => { gone = true }
  })

  const toggleTag = (id: number) => { if (choice) choice.tags = choice.tags.includes(id) ? choice.tags.filter((t) => t !== id) : [...choice.tags, id] }

  async function send() {
    busy = true
    error = ''
    try {
      ondone(await entracteApi.accept(request.id, choice ?? undefined))
      onclose()
    } catch (e) {
      error = messageOf(e, 'La demande n’a pas pu être acceptée.')
    } finally {
      busy = false
    }
  }
</script>

<Modal title="Accepter « {request.title} »" {onclose}>
  <form class="acc" onsubmit={(e) => { e.preventDefault(); void send() }}>
    {#if unavailable}
      <div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{unavailable} La demande peut être envoyée avec les réglages habituels de {service}.</p></div>
    {:else if !options || !choice}
      <Loader />
    {:else}
      <p class="v-note">Réglages repris de {service} : ceux que ses titres utilisent déjà le plus. Modifie-les pour ce titre si besoin.</p>
      <label class="acc-field"><span>Emplacement</span>
        <select class="plbls-select" bind:value={choice.folder}>
          {#each options.folders as f (f.path)}<option value={f.path}>{f.path}{freeSpace(f.free) ? ` · ${freeSpace(f.free)}` : ''}</option>{/each}
        </select>
      </label>
      <label class="acc-field"><span>Profil de qualité</span>
        <select class="plbls-select" bind:value={choice.profile}>
          {#each options.profiles as p (p.id)}<option value={p.id}>{p.name || `Profil ${p.id}`}</option>{/each}
        </select>
      </label>
      {#if options.tags.length}
        <fieldset class="acc-tags">
          <legend>Tags</legend>
          {#each options.tags as t (t.id)}
            <label class="acc-tag"><input type="checkbox" checked={choice.tags.includes(t.id)} onchange={() => toggleTag(t.id)} /> {t.label || `Tag ${t.id}`}</label>
          {/each}
        </fieldset>
        <p class="v-note">Dans {service}, un indexeur ou un client de téléchargement qui porte des tags ne sert qu’aux titres qui en partagent un.</p>
      {/if}
    {/if}
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    <div class="acc-actions">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button type="submit" class="plbls-button" disabled={busy || (!choice && !unavailable)}><Icon name="envoi" size={16} /> Accepter et envoyer à {service}</button>
    </div>
  </form>
</Modal>

<style>
  .acc { display: flex; flex-direction: column; gap: var(--espace-3); }
  .acc-field { display: flex; flex-direction: column; gap: var(--espace-1); }
  .acc-field > span, .acc-tags legend { font-size: 13px; font-weight: 600; }
  .acc-tags { margin: 0; padding: 0; border: 0; display: flex; flex-wrap: wrap; gap: var(--espace-1) var(--espace-3); }
  .acc-tags legend { padding: 0; margin-bottom: var(--espace-1); }
  .acc-tag { display: inline-flex; align-items: center; gap: var(--espace-1); min-height: 32px; }
  .acc-actions { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: var(--espace-2); }
</style>
