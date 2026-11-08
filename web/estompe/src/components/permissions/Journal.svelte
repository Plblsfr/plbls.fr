<script lang="ts">
  import { charger, ton, type PermAudit } from '../../lib/permissions.svelte'
  import Attente from './Attente.svelte'

  const lignes = charger<PermAudit[]>(() => '/audit?limit=200')
</script>

<section class="v-card perm-carte">
  <h2 class="perm-t">Journal des permissions</h2>
  {#if lignes.data === null}
    <Attente error={lignes.error} retry={lignes.reload} />
  {:else if lignes.data.length === 0}
    <p class="v-note">Aucune modification enregistrée.</p>
  {:else}
    <div class="perm-defile">
      <table class="perm-tb">
        <thead><tr><th scope="col">Date</th><th scope="col">Acteur</th><th scope="col">Action</th><th scope="col">Cible</th><th scope="col">Détail</th></tr></thead>
        <tbody>
          {#each lignes.data as r, i (i)}
            <tr>
              <td class="v-note">{new Date(r.at).toLocaleString('fr-FR')}</td><td class="v-mono">{r.actor}</td>
              <td><span class="v-tag {ton(r.action)}">{r.action}</span></td>
              <td class="v-mono">{r.target}</td><td class="v-mono">{JSON.stringify(r.detail)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>
