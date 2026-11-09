<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { ecrire, executer, libellesPermissions, quand, typeDe, type Catalogue, type RegleVue } from '../../lib/acces.svelte'
  import NomRessource from './NomRessource.svelte'

  let { regles, catalogue, onChange, vide = 'Aucune règle.' }: { regles: RegleVue[]; catalogue: Catalogue; onChange: () => void; vide?: string } = $props()
  const noms = $derived(libellesPermissions(catalogue))

  const supprimer = async (r: RegleVue) => {
    const nom = noms.get(r.permission)?.label ?? r.permission
    if (!(await confirmer({ titre: 'Supprimer la règle', message: `${r.effect === 'allow' ? 'Retirer' : 'Lever le refus de'} « ${nom} »${r.resource !== '*' ? ` sur ${r.resource}` : ''} ?`, action: 'Supprimer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/rules/${r.id}`), 'Règle supprimée')) !== null) onChange()
  }
</script>

{#if regles.length === 0}
  <p class="v-note">{vide}</p>
{:else}
  <div class="perm-defile">
    <table class="perm-tb">
      <thead><tr><th scope="col">Permission</th><th scope="col">Effet</th><th scope="col">Portée</th><th scope="col">Expire</th><th scope="col">Note</th><th scope="col">Par</th><th scope="col"><span class="perm-sr">Actions</span></th></tr></thead>
      <tbody>
        {#each regles as r (r.id)}
          <tr class:acc-obsolete={r.obsolete}>
            <td><b>{noms.get(r.permission)?.label ?? r.permission}</b><br /><span class="v-mono v-mute">{r.permission}</span></td>
            <td><span class="v-tag {r.effect === 'allow' ? 'ok' : 'dgr'}">{r.effect === 'allow' ? 'Accordée' : 'Refusée'}</span></td>
            <td><NomRessource type={typeDe(noms, r.permission)} id={r.resource} /></td>
            <td class="v-note">{r.expires_at ? quand(r.expires_at) : 'jamais'}</td>
            <td class="v-note">{r.note || '—'}</td>
            <td class="v-mono v-mute">{r.granted_by}</td>
            <td class="perm-act"><button class="perm-suppr" onclick={() => supprimer(r)}>Supprimer</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
