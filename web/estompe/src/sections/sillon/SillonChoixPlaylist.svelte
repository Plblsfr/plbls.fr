<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Modal from '../../components/Modal.svelte'
  import SillonMosaique from './SillonMosaique.svelte'
  import { choix, collection } from './sillon.svelte'
  import { titresDe } from './sillon'

  let envoi = $state(false)
  const fermer = () => { choix.contenu = null }

  $effect(() => { if (choix.contenu && !collection.charge) void collection.charger() })

  async function vers(id: string) {
    const c = choix.contenu
    if (!c || envoi) return
    envoi = true
    try { if (await collection.ajouter(id, c)) fermer() } finally { envoi = false }
  }

  async function nouvelle() {
    const c = choix.contenu
    fermer()
    const p = await collection.creer()
    if (p && c) await collection.ajouter(p.id, c)
  }
</script>

{#if choix.contenu}
  <Modal title={`Ajouter « ${choix.titre} » à une playlist`} onclose={fermer}>
    <div class="sillon-choix">
      <button class="sillon-choix-l" onclick={() => void nouvelle()}><span class="sillon-choix-art sillon-choix-plus"><Icon name="plus" size={20} /></span><strong>Nouvelle playlist</strong></button>
      {#each collection.playlists as p (p.id)}
        <button class="sillon-choix-l" disabled={envoi} onclick={() => void vers(p.id)}>
          <span class="sillon-choix-art"><SillonMosaique albums={p.cover_albums} id={p.id} titre={p.name} size="small" /></span>
          <span class="grow sillon-choix-t"><strong class="truncate">{p.name}</strong><span class="v-note">{titresDe(p.track_count)}</span></span>
        </button>
      {/each}
    </div>
  </Modal>
{/if}

<style>
  .sillon-choix { display: flex; flex-direction: column; gap: 2px; max-height: 60vh; overflow-y: auto; }
  .sillon-choix-l { display: flex; align-items: center; gap: 12px; width: 100%; min-height: 52px; padding: 6px 8px; border: 0; border-radius: var(--r); background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .sillon-choix-l:hover:not(:disabled) { background: var(--papier-2); }
  .sillon-choix-l:focus-visible { outline: none; box-shadow: var(--anneau); }
  .sillon-choix-art { position: relative; display: block; flex: none; width: 44px; height: 44px; overflow: hidden; border-radius: var(--r-s); }
  .sillon-choix-plus { display: grid; place-items: center; background: var(--papier-2); }
  .sillon-choix-t { display: grid; min-width: 0; }
</style>
