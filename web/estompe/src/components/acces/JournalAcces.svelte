<script lang="ts">
  import Attente from '../permissions/Attente.svelte'
  import { ACTIONS, charger, enc, libellesPermissions, type Catalogue, type EntreeJournal } from '../../lib/acces.svelte'
  import LigneJournal from './LigneJournal.svelte'

  let { catalogue }: { catalogue: Catalogue } = $props()
  let action = $state('')
  const lignes = charger<EntreeJournal[]>(() => `/audit?limit=300${action ? `&action=${enc(action)}` : ''}`)
  const noms = $derived(libellesPermissions(catalogue))
</script>

<section class="v-card perm-carte">
  <div class="perm-tete">
    <h2 class="perm-t2">Journal des accès</h2>
    <div class="plbls-field">
      <label class="plbls-field__label" for="acc-j-a">Action</label>
      <select class="plbls-select" id="acc-j-a" bind:value={action}>
        <option value="">Toutes</option>
        {#each Object.entries(ACTIONS) as [k, l] (k)}<option value={k}>{l}</option>{/each}
      </select>
    </div>
  </div>
  {#if lignes.data === null}<Attente error={lignes.error} retry={lignes.reload} />
  {:else if lignes.data.length === 0}<p class="v-note">Aucune modification.</p>
  {:else}<ul class="acc-journal">{#each lignes.data as l (l.id)}<LigneJournal ligne={l} {noms} />{/each}</ul>{/if}
</section>
