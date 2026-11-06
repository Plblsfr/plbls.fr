<script lang="ts">
  import Loader from '../../components/Loader.svelte'
  import { cairnApi } from '../../lib/api'
  import { cairn } from './cairn.svelte'
  import { cairnError, type TrashItem } from './cairn'

  let trash = $state<TrashItem[] | null>(null)
  let error = $state('')
  const when = (d: string) => new Date(d).toLocaleString('fr-FR')
  const load = () => cairnApi.trash(cairn.space).then((l) => { trash = l; error = '' }).catch((e) => (error = cairnError(e)))
  void load()

  async function restore(id: string) {
    try { await cairnApi.restorePage(id); await Promise.all([load(), cairn.loadPages()]) } catch (e) { error = cairnError(e) }
  }
</script>

<div class="cx-page">
  <div class="cx-col">
    <h2 class="cx-h">Corbeille</h2>
    <p class="muted small">Les pages supprimées restent 30 jours ici avant d'être effacées.</p>
    {#if error}<p class="cx-err small" role="alert">{error}</p>{/if}
    {#if trash === null}
      {#if !error}<Loader />{/if}
    {:else}
      <ul class="cx-list">
        {#each trash as t (t.id)}
          <li class="cx-item"><span class="cx-title">{t.title} <span class="muted small">supprimée le {when(t.deleted_at)}</span></span><button class="plbls-button plbls-button--compact" onclick={() => restore(t.id)}>Restaurer</button></li>
        {:else}
          <li class="plbls-empty-state"><h3>Corbeille vide</h3><p>Rien à restaurer.</p></li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
