<script lang="ts">
  import { confirmer } from '../lib/confirm.svelte'
  import Modal from './Modal.svelte'
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
  <div class="stack">
    {#if canSettings}
      <form class="stack" onsubmit={(e) => { e.preventDefault(); void save() }}>
        <div class="plbls-field">
          <label class="plbls-field__label" for="c-slow">Mode lent</label>
          <select id="c-slow" class="plbls-select" bind:value={slowmode}>{#each SLOW as [label, v]}<option value={v}>{label}</option>{/each}</select>
          <p class="muted small">Délai minimum entre deux messages d'un même membre, sauf pour ceux qui peuvent gérer les messages.</p>
        </div>
        <label class="row"><input type="checkbox" bind:checked={locked} /><span>Verrouiller le salon (lecture seule pour les membres sans « Gérer les messages »)</span></label>
        <div><button class="plbls-button" disabled={busy}>Enregistrer</button></div>
      </form>
    {/if}
    {#if canClean}
      <form class="stack" onsubmit={(e) => { e.preventDefault(); clean() }}>
        <strong>Supprimer des messages</strong>
        <div class="row">
          <div class="plbls-field">
            <label class="plbls-field__label" for="c-last">Les N derniers (1 à 100)</label>
            <input id="c-last" class="plbls-input" type="number" min="1" max="100" bind:value={last} required />
          </div>
          <div class="plbls-field grow">
            <label class="plbls-field__label" for="c-author">Auteur</label>
            <select id="c-author" class="plbls-select" bind:value={author}>
              <option value="">Tout le monde</option>
              {#each detail?.members ?? [] as m (m.id)}<option value={m.id}>{m.pseudo}</option>{/each}
            </select>
          </div>
        </div>
        <div><button class="plbls-button plbls-button--danger" disabled={busy || !(last >= 1 && last <= 100)}>Supprimer</button></div>
      </form>
    {/if}
  </div>
</Modal>
