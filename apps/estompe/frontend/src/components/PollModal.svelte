<script lang="ts">
  import Icon from './Icon.svelte'
  import Modal from './Modal.svelte'
  import { app } from '../lib/store.svelte'
  import { notify } from '../lib/ui'

  let { convKey, onclose }: { convKey: string; onclose: () => void } = $props()

  const DURATIONS = [['1 h', 1], ['24 h', 24], ['3 jours', 72], ['1 semaine', 168], ['Sans clôture', 0], ['Personnalisée', -1]] as const
  let question = $state('')
  let options = $state(['', ''])
  let multiple = $state(false)
  let hours = $state(24)
  let custom = $state('')
  let sending = $state(false)

  const ok = $derived(!sending && question.trim().length > 0 && options.filter((o) => o.trim()).length >= 2 && (hours >= 0 || (!!custom && new Date(custom) > new Date())))

  async function submit() {
    if (!ok) return
    sending = true
    try {
      const end = hours > 0 ? new Date(Date.now() + hours * 3600e3) : hours < 0 ? new Date(custom) : undefined
      await app.send(convKey, question.trim(), [], undefined, { options: options.map((o) => o.trim()).filter(Boolean), multiple, ...(end ? { closes_at: end.toISOString() } : {}) })
      onclose()
    } catch (e) {
      notify.error(e, "Le sondage n'a pas pu être publié")
    } finally {
      sending = false
    }
  }
</script>

<Modal title="Nouveau sondage" {onclose}>
  <form class="poll-form" onsubmit={(e) => { e.preventDefault(); void submit() }}>
    <input class="plbls-input" bind:value={question} maxlength="500" placeholder="Votre question" aria-label="Question" />
    {#each options as _, i}
      <div class="row">
        <input class="plbls-input grow" bind:value={options[i]} maxlength="100" placeholder="Choix {i + 1}" aria-label="Choix {i + 1}" />
        {#if options.length > 2}<button type="button" class="ib sm" onclick={() => options.splice(i, 1)} aria-label="Supprimer le choix {i + 1}"><Icon name="x" size={14} /></button>{/if}
      </div>
    {/each}
    {#if options.length < 10}<button type="button" class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => options.push('')}>Ajouter un choix</button>{/if}
    <label class="row"><input type="checkbox" bind:checked={multiple} /> Autoriser plusieurs réponses</label>
    <div class="row small poll-durations" role="radiogroup" aria-label="Clôture">
      {#each DURATIONS as [label, h]}
        <button type="button" class="plbls-button plbls-button--compact {hours === h ? '' : 'plbls-button--contour'}" role="radio" aria-checked={hours === h} onclick={() => (hours = h)}>{label}</button>
      {/each}
    </div>
    {#if hours < 0}<input class="plbls-input" type="datetime-local" bind:value={custom} aria-label="Date de clôture" />{/if}
    <div style="display: flex; gap: 8px; justify-content: flex-end">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button class="plbls-button" disabled={!ok}>Publier le sondage</button>
    </div>
  </form>
</Modal>
