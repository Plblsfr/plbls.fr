<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import Modal from '../../components/Modal.svelte'
  import { notify } from '../../lib/ui'
  import { LIBRARIES, bytes, regieApi, serviceName, type Deposit, type ImportProposal, type LibraryKey, type LibraryTitle } from './regie'

  let { deposit, onclose, ondone }: { deposit: Deposit; onclose: () => void; ondone: () => void } = $props()

  let service = $state<LibraryKey>('radarr')
  let proposal = $state<ImportProposal | null>(null)
  let failed = $state('')
  let busy = $state(false)
  let query = $state('')
  let found = $state<LibraryTitle[] | null>(null)
  let chosen = $state<LibraryTitle | null>(null)
  let tour = 0

  const recognized = $derived(proposal?.items.filter((i) => i.recognized) ?? [])
  const count = $derived(chosen ? (proposal?.items.length ?? 0) : recognized.length)

  $effect(() => {
    const s = service
    const t = ++tour
    proposal = null
    failed = ''
    chosen = null
    found = null
    query = ''
    regieApi.importProposal(deposit.id, s).then((p) => { if (t === tour) proposal = p }).catch((e) => { if (t === tour) failed = e instanceof Error ? e.message : String(e) })
  })

  async function search(e: SubmitEvent) {
    e.preventDefault()
    if (!query.trim()) return
    try { found = (await regieApi.library('radarr', query.trim())).items } catch (err) { notify.error(err, 'La recherche a échoué') }
  }

  async function confirm() {
    busy = true
    try {
      const r = await regieApi.importDeposit(deposit.id, service, chosen?.id)
      notify.ok(`${r.files} fichier${r.files > 1 ? 's' : ''} confié${r.files > 1 ? 's' : ''} à ${serviceName(r.service)}, qui termine l’import.`)
      ondone()
      onclose()
    } catch (e) { notify.error(e, 'L’import n’a pas pu être lancé') } finally { busy = false }
  }
</script>

<Modal title="Importer « {deposit.name} »" wide {onclose}>
  <div class="rg-import">
    <div class="v-seg" role="group" aria-label="Bibliothèque">
      {#each LIBRARIES as l (l.key)}<button aria-pressed={service === l.key} onclick={() => (service = l.key)}>{l.label}</button>{/each}
    </div>

    {#if failed}
      <p class="v-note rg-err" role="alert"><Icon name="alerte" size={14} /> {failed}</p>
    {:else if !proposal}
      <Loader />
    {:else if !proposal.items.length}
      <p class="v-note">{serviceName(service)} ne voit aucun fichier dans ce dépôt. Vérifie que le dossier des dépôts est monté chez lui.</p>
    {:else}
      <ul class="rg-items">
        {#each proposal.items as i (i.path)}
          <li>
            <span class="grow rg-body">
              <span class="rg-wrap">{i.path}</span>
              {#if chosen}
                <span class="v-note">→ {chosen.title}{chosen.year ? ` (${chosen.year})` : ''}</span>
              {:else if i.recognized && i.title}
                <span class="v-note">→ {[i.title.title, i.title.detail].filter(Boolean).join(' · ')}</span>
              {:else}
                <span class="v-note rg-warn">Non reconnu{i.reasons.length ? ` : ${i.reasons.join(' ; ')}` : ''}</span>
              {/if}
            </span>
            <span class="v-note">{bytes(i.size)}</span>
          </li>
        {/each}
      </ul>

      {#if service === 'radarr'}
        <form class="rg-cherche" onsubmit={search}>
          <span class="film-search grow"><Icon name="cherche" size={16} /><input class="plbls-input" type="search" aria-label="Chercher un film de la bibliothèque" placeholder="Corriger : chercher un film de Radarr" maxlength="80" bind:value={query} /></span>
          <button class="plbls-button plbls-button--compact plbls-button--contour">Chercher</button>
          {#if chosen}<button type="button" class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (chosen = null)}>Revenir à la proposition</button>{/if}
        </form>
        {#if found}
          {#if !found.length}
            <p class="v-note">Aucun film de ce nom dans Radarr. Un titre absent se demande d’abord dans Entracte.</p>
          {:else}
            <div class="rg-choix" role="group" aria-label="Films trouvés">
              {#each found as t (t.id)}<button class="v-tag" class:encre={chosen?.id === t.id} aria-pressed={chosen?.id === t.id} onclick={() => (chosen = t)}>{t.title}{t.year ? ` (${t.year})` : ''}</button>{/each}
            </div>
          {/if}
        {/if}
      {:else if !recognized.length}
        <p class="v-note">Le titre doit déjà exister dans {serviceName(service)} pour recevoir ces fichiers.</p>
      {/if}
    {/if}

    <div class="rg-actions">
      <button class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button class="plbls-button" disabled={busy || !count} onclick={() => void confirm()}>Importer {count || ''} fichier{count > 1 ? 's' : ''}</button>
    </div>
  </div>
</Modal>

<style>
  .rg-import { display: flex; flex-direction: column; gap: var(--espace-3); }
  .rg-import .v-note { margin: 0; }
  .rg-items { list-style: none; margin: 0; padding: 0; max-height: 36vh; overflow-y: auto; }
  .rg-items li { display: flex; align-items: center; gap: var(--espace-2); padding: var(--espace-1) 0; box-shadow: inset 0 -1px 0 var(--trait); }
  .rg-body { display: flex; flex-direction: column; min-width: 0; }
  .rg-wrap { overflow-wrap: anywhere; }
  .rg-cherche { display: flex; flex-wrap: wrap; align-items: center; gap: var(--espace-2); }
  .rg-choix { display: flex; flex-wrap: wrap; gap: var(--espace-1); }
  .rg-choix button { border: 0; cursor: pointer; font: inherit; font-size: 12px; height: 26px; }
  .rg-actions { display: flex; justify-content: flex-end; gap: var(--espace-2); }
  .rg-err { display: flex; align-items: center; gap: 4px; color: var(--danger); }
  .rg-warn { color: var(--attention); }
</style>
