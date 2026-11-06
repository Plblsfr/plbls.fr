<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { api, ApiError } from '../lib/api'
  import { router, hrefs } from '../lib/route.svelte'
  import { messageOf } from '../lib/ui'
  import Modal from './Modal.svelte'

  let { onclose }: { onclose: () => void } = $props()

  let pseudo = $state('')
  let error = $state('')
  let busy = $state(false)

  // Pseudos déjà croisés (membres de tes serveurs, conversations) pour l'autocomplétion.
  const known = $derived(
    [...new Set([...Object.values(app.details).flatMap((d) => d.members.map((m) => m.pseudo)), ...app.dms.map((d) => d.other.pseudo)])]
      .filter((p) => p !== app.pseudo)
      .sort((a, b) => a.localeCompare(b, 'fr', { sensitivity: 'base' })),
  )

  async function submit(e: Event) {
    e.preventDefault()
    busy = true
    error = ''
    try {
      const dm = await api.openDM(pseudo.trim())
      if (dm.online) app.online[dm.other.id] = true
      if (!app.dms.some((d) => d.id === dm.id)) app.dms.unshift(dm)
      onclose()
      router.go(hrefs.dm(dm.id))
    } catch (err) {
      error = err instanceof ApiError && err.code === 'user_not_found' ? 'Aucun utilisateur avec ce pseudo.'
        : err instanceof ApiError && err.code === 'self_dm' ? 'Tu ne peux pas t\'écrire à toi-même.'
        : messageOf(err)
    } finally {
      busy = false
    }
  }
</script>

<Modal title="Nouveau message privé" {onclose}>
  <form class="stack" onsubmit={submit}>
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    <div class="plbls-field">
      <label class="plbls-field__label" for="dm-pseudo">Pseudo</label>
      <input id="dm-pseudo" class="plbls-input" list="dm-known" bind:value={pseudo} autocomplete="off" required />
      <datalist id="dm-known">{#each known as p}<option value={p}></option>{/each}</datalist>
    </div>
    <button class="plbls-button" disabled={busy || !pseudo.trim()}>Ouvrir la conversation</button>
  </form>
</Modal>
