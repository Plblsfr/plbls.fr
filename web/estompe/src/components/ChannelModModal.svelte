<script lang="ts">
  import { confirmer } from '../lib/confirm.svelte'
  import Modal from './Modal.svelte'
  import Icon from './Icon.svelte'
  import { api } from '../lib/api'
  import { app } from '../lib/store.svelte'
  import { notify } from '../lib/ui'
  import { SLOW } from '../lib/channel-create.svelte'

  let { serverId, channelId, onclose }: { serverId: string; channelId: string; onclose: () => void } = $props()

  const detail = $derived(app.details[serverId])
  const channel = $derived(detail?.channels.find((c) => c.id === channelId))
  const canSettings = $derived(app.can(serverId, 'manage_channels'))
  const canClean = $derived(app.can(serverId, 'manage_messages', channelId))

  // Valeurs initiales volontaires : les champs sont ensuite libres jusqu'à l'enregistrement.
  // svelte-ignore state_referenced_locally
  let slowmode = $state(channel?.slowmode ?? 0)
  // svelte-ignore state_referenced_locally
  let locked = $state(channel?.locked ?? false)
  let last = $state(10)
  let author = $state('')
  let busy = $state(false)

  async function run(fn: () => Promise<unknown>, ok: string) {
    busy = true
    try {
      await fn()
      notify.ok(ok)
    } catch (e) {
      notify.error(e, 'Action impossible')
    } finally {
      busy = false
    }
  }
  const save = () => run(() => api.channelModeration(serverId, channelId, slowmode, locked), 'Salon mis à jour')
  const clean = async () => {
    if (!(await confirmer({ titre: 'Supprimer des messages', message: `Supprimer les ${last} derniers messages${author ? ' de ce membre' : ''} de « ${channel?.name} » ? Cette action est définitive.`, action: 'Supprimer les messages', destructif: true }))) return
    void run(async () => { const r = await api.bulkDelete(channelId, { last, author: author || undefined }); notify.info(`${r.deleted.length} message(s) supprimé(s)`) }, 'Messages supprimés')
  }
</script>

<Modal title="Modération · {channel?.name ?? ''}" {onclose}>
  <div class="cm">
    {#if canSettings}
      <form class="cm-sec" onsubmit={(e) => { e.preventDefault(); void save() }}>
        <div class="cm-row">
          <label class="v-lbl grow" for="c-slow">Mode lent</label>
          <select id="c-slow" class="plbls-select cm-sel" bind:value={slowmode} aria-describedby="c-slow-aide">{#each SLOW as [label, v]}<option value={v}>{label}</option>{/each}</select>
        </div>
        <p id="c-slow-aide" class="v-note">Délai minimum entre deux messages d'un même membre, sauf pour ceux qui peuvent gérer les messages.</p>
        <label class="plbls-toggle">
          <input class="plbls-toggle__input" type="checkbox" bind:checked={locked} />
          <span class="plbls-toggle__piste"></span>
          <span class="plbls-toggle__texte">Verrouiller le salon (lecture seule pour les membres sans « Gérer les messages »)</span>
        </label>
        <div class="cm-f"><button class="plbls-button plbls-button--compact" disabled={busy}>Enregistrer</button></div>
      </form>
    {/if}
    {#if canSettings && canClean}<hr class="v-hr" />{/if}
    {#if canClean}
      <form class="cm-sec" onsubmit={(e) => { e.preventDefault(); clean() }}>
        <strong class="cm-t">Supprimer des messages</strong>
        <div class="cm-clean">
          <div class="plbls-field">
            <label class="plbls-field__label" for="c-last">Les N derniers (1 à 100)</label>
            <input id="c-last" class="plbls-input v-mono cm-n" type="number" min="1" max="100" bind:value={last} required />
          </div>
          <div class="plbls-field grow">
            <label class="plbls-field__label" for="c-author">Auteur</label>
            <select id="c-author" class="plbls-select" bind:value={author}>
              <option value="">Tout le monde</option>
              {#each detail?.members ?? [] as m (m.id)}<option value={m.id}>{m.pseudo}</option>{/each}
            </select>
          </div>
        </div>
        <div class="cm-f"><button class="plbls-button plbls-button--compact plbls-button--danger" disabled={busy || !(last >= 1 && last <= 100)}><Icon name="corbeille" size={16} />Supprimer</button></div>
      </form>
    {/if}
  </div>
</Modal>

<style>
  .cm { display: flex; flex-direction: column; gap: 16px; }
  .cm-sec { display: flex; flex-direction: column; gap: 12px; }
  .cm-sec p { margin: 0; }
  .cm-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .cm-sel { width: 160px; }
  .cm-t { font-family: var(--f-titre); font-size: 16px; }
  .cm-clean { display: flex; align-items: flex-end; gap: 10px; flex-wrap: wrap; }
  .cm-n { width: 110px; }
  .cm-f { display: flex; justify-content: flex-end; }
</style>
