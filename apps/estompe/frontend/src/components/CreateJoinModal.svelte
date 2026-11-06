<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { api, ApiError } from '../lib/api'
  import { router, hrefs } from '../lib/route.svelte'
  import { messageOf, notify } from '../lib/ui'
  import Modal from './Modal.svelte'

  let { onclose }: { onclose: () => void } = $props()

  let name = $state('')
  let invite = $state('')
  let busy = $state(false)
  let error = $state('')

  /** Accepte un lien complet (…/#/join/CODE) ou le code seul. */
  function codeFrom(input: string): string {
    const m = /([A-Za-z0-9]{5}-[A-Za-z0-9]{5})\s*$/.exec(input.trim())
    return (m ? m[1] : input.trim()).toUpperCase()
  }

  async function create(e: Event) {
    e.preventDefault()
    busy = true
    error = ''
    try {
      const s = await api.createServer(name.trim())
      await app.refreshServers()
      onclose()
      router.go(hrefs.channel(s.id))
    } catch (err) {
      error = messageOf(err)
    } finally {
      busy = false
    }
  }

  async function join(e: Event) {
    e.preventDefault()
    busy = true
    error = ''
    try {
      const s = await api.joinServer(codeFrom(invite))
      await app.refreshServers()
      onclose()
      router.go(hrefs.channel(s.id))
      notify.ok(`Tu as rejoint « ${s.name} »`)
    } catch (err) {
      error = err instanceof ApiError && err.status === 404 ? 'Invitation invalide ou révoquée.' : messageOf(err)
    } finally {
      busy = false
    }
  }
</script>

<Modal title="Créer ou rejoindre un serveur" {onclose}>
  <div class="stack">
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    <form class="stack" onsubmit={create}>
      <div class="plbls-field">
        <label class="plbls-field__label" for="srv-name">Créer un serveur</label>
        <input id="srv-name" class="plbls-input" bind:value={name} maxlength="50" placeholder="Nom du serveur" required />
      </div>
      <button class="plbls-button" disabled={busy || !name.trim()}>Créer</button>
    </form>
    <hr style="border: 0; border-top: 1px solid var(--trait); width: 100%;" />
    <form class="stack" onsubmit={join}>
      <div class="plbls-field">
        <label class="plbls-field__label" for="srv-code">Rejoindre avec un lien ou un code</label>
        <input id="srv-code" class="plbls-input mono" bind:value={invite} placeholder="ABCDE-23456" autocomplete="off" required />
      </div>
      <button class="plbls-button plbls-button--contour" disabled={busy || !invite.trim()}>Rejoindre</button>
    </form>
  </div>
</Modal>
