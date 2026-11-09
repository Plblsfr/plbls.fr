<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { ecrire, executer, quand, type LiaisonVue } from '../../lib/acces.svelte'
  import NomRessource from './NomRessource.svelte'

  let { liaisons, onChange, sujet = false, lecture = false, vide = 'Aucun rôle.' }: { liaisons: LiaisonVue[]; onChange: () => void; sujet?: boolean; lecture?: boolean; vide?: string } = $props()

  const retirer = async (l: LiaisonVue) => {
    if (!(await confirmer({ titre: 'Retirer le rôle', message: `Retirer le rôle « ${l.role} » à ${l.subject_label ?? l.subject_id} ?`, action: 'Retirer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/bindings/${l.id}`), 'Rôle retiré')) !== null) onChange()
  }
</script>

{#if liaisons.length === 0}
  <p class="v-note">{vide}</p>
{:else}
  <div class="perm-defile">
    <table class="perm-tb">
      <thead><tr>{#if sujet}<th scope="col">Compte ou groupe</th>{:else}<th scope="col">Rôle</th>{/if}<th scope="col">Portée</th><th scope="col">Expire</th><th scope="col">Depuis</th>{#if !lecture}<th scope="col"><span class="perm-sr">Actions</span></th>{/if}</tr></thead>
      <tbody>
        {#each liaisons as l (l.id)}
          <tr>
            {#if sujet}<td><b>{l.subject_label ?? l.subject_id}</b> <span class="v-tag">{l.subject_kind === 'group' ? 'groupe' : 'compte'}</span></td>{:else}<td><span class="v-tag rose">{l.role}</span></td>{/if}
            <td>{#if l.scope_id}<NomRessource type={l.scope_type} id={l.scope_id} />{:else}partout{/if}</td>
            <td class="v-note">{l.expires_at ? quand(l.expires_at) : 'jamais'}</td>
            <td class="v-note">{quand(l.created_at)}</td>
            {#if !lecture}<td class="perm-act"><button class="perm-suppr" onclick={() => retirer(l)}>Retirer</button></td>{/if}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
