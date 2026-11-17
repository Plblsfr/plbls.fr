<script lang="ts">
  import { untrack } from 'svelte'
  import Modal from '../../components/Modal.svelte'
  import { notify } from '../../lib/ui'
  import { limitBytes, limitInput, regieApi, type GlobalTransfer } from './regie'

  let { transfer, onclose }: { transfer: GlobalTransfer; onclose: () => void } = $props()

  const initial = untrack(() => transfer)
  let download = $state(limitInput(initial.download_limit))
  let upload = $state(limitInput(initial.upload_limit))
  let alternative = $state(initial.alternative)
  let busy = $state(false)
  const d = $derived(limitBytes(download))
  const u = $derived(limitBytes(upload))

  async function save(e: SubmitEvent) {
    e.preventDefault()
    if (d === null || u === null) return
    busy = true
    try { await regieApi.setTransfer(d, u, alternative); notify.ok('Limites enregistrées.'); onclose() } catch (err) { notify.error(err, 'Les limites n’ont pas pu être enregistrées') } finally { busy = false }
  }
</script>

<Modal title="Limites de débit" {onclose}>
  <form class="rg-form" onsubmit={save}>
    <p class="v-note">Limites globales de qBittorrent, en Ko/s. Vide : sans limite.{transfer.alternative ? ' Les valeurs affichées sont celles du mode alternatif, actif.' : ''}</p>
    <label class="plbls-field"><span>Réception (Ko/s)</span><input class="plbls-input" inputmode="decimal" aria-invalid={d === null} bind:value={download} /></label>
    <label class="plbls-field"><span>Envoi (Ko/s)</span><input class="plbls-input" inputmode="decimal" aria-invalid={u === null} bind:value={upload} /></label>
    <label class="rg-case"><input type="checkbox" bind:checked={alternative} /> Mode alternatif (limites réduites réglées dans qBittorrent)</label>
    <div class="rg-actions">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button class="plbls-button" disabled={busy || d === null || u === null}>Enregistrer</button>
    </div>
  </form>
</Modal>

<style>
  .rg-form { display: flex; flex-direction: column; gap: var(--espace-3); }
  .rg-form .v-note { margin: 0; }
  .rg-case { display: flex; align-items: center; gap: var(--espace-2); }
  .rg-actions { display: flex; justify-content: flex-end; gap: var(--espace-2); }
</style>
