<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { api, ApiError } from '../lib/api'
  import { router, hrefs } from '../lib/route.svelte'
  import { messageOf, notify } from '../lib/ui'
  import Modal from './Modal.svelte'
  import Icon from './Icon.svelte'
  import { initials } from '../lib/format'

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
  <div class="cj">
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    <div class="cj-choix">
      {#if app.peut('servers.create')}<form class="cj-bloc" onsubmit={create}>
        <span class="cj-icone rose" aria-hidden="true"><Icon name="plus" size={22} /></span>
        <div class="plbls-field">
          <label class="plbls-field__label cj-titre" for="srv-name">Créer un serveur</label>
          <input id="srv-name" class="plbls-input" bind:value={name} maxlength="50" placeholder="Nom du serveur" required />
        </div>
        {#if name.trim()}
          <div class="cj-apercu v-note"><span class="cj-pave" aria-hidden="true">{initials(name.trim())}</span>Aperçu dans la barre des serveurs</div>
        {/if}
        <button class="plbls-button plbls-button--block" disabled={busy || !name.trim()}>Créer</button>
      </form>{/if}
      {#if app.peut('servers.join')}<form class="cj-bloc" onsubmit={join}>
        <span class="cj-icone" aria-hidden="true"><Icon name="lien" size={22} /></span>
        <div class="plbls-field">
          <label class="plbls-field__label cj-titre" for="srv-code">Rejoindre avec un lien ou un code</label>
          <input id="srv-code" class="plbls-input cj-code" bind:value={invite} placeholder="ABCDE-23456" autocomplete="off" aria-describedby="srv-code-aide" required />
          <span class="plbls-field__aide" id="srv-code-aide">Colle un lien d'invitation complet, ou juste le code.</span>
        </div>
        <button class="plbls-button plbls-button--contour plbls-button--block" disabled={busy || !invite.trim()}>Rejoindre</button>
      </form>{/if}
    </div>
  </div>
</Modal>

<style>
  .cj { display: flex; flex-direction: column; gap: 16px; }
  .cj .plbls-alert, .cj .plbls-field { margin: 0; }
  .cj-choix { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 220px), 1fr)); gap: 16px; }
  .cj-bloc { display: flex; flex-direction: column; gap: 12px; padding: 18px; border-radius: var(--r); background: var(--papier-2); }
  .cj-bloc > button { margin-top: auto; }
  .cj-icone { display: inline-grid; place-items: center; width: 44px; height: 44px; border-radius: 14px; background: var(--av-3); color: var(--av-3-t); }
  .cj-icone.rose { background: var(--rose); color: var(--encre); }
  .cj-titre { font-family: var(--f-titre); font-size: 18px; font-weight: 700; letter-spacing: -0.02em; color: var(--encre); }
  .cj-code { font-family: var(--f-mono); letter-spacing: 0.02em; }
  .cj-apercu { display: flex; align-items: center; gap: 8px; }
  .cj-pave { display: inline-grid; place-items: center; width: 32px; height: 32px; border-radius: 10px; background: var(--encre); color: var(--papier); font-family: var(--f-titre); font-size: 11px; font-weight: 700; }
  @media (pointer: coarse) {
    .cj-bloc .plbls-button { min-height: 44px; }
  }
</style>
