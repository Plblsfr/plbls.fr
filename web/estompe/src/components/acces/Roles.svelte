<script lang="ts">
  import Attente from '../permissions/Attente.svelte'
  import { charger, ecrire, executer, type Catalogue, type RoleDetail, type RoleResume } from '../../lib/acces.svelte'
  import RoleEditeur from './RoleEditeur.svelte'

  let { catalogue }: { catalogue: Catalogue } = $props()
  const roles = charger<RoleResume[]>(() => '/roles')
  let choisi = $state<string | null>(null)
  let nom = $state('')
  const creer = async (e: SubmitEvent) => {
    e.preventDefault()
    const r = await executer(ecrire<RoleDetail>('POST', '/roles', { name: nom, rules: [] }), 'Rôle créé')
    if (r) { nom = ''; roles.reload(); choisi = r.id }
  }
</script>

<div class="perm-cols acc-cols">
  <section class="v-card perm-carte">
    <h2 class="perm-t2">Rôles</h2>
    {#if roles.data === null}
      <Attente error={roles.error} retry={roles.reload} />
    {:else}
      <div class="perm-lignes">
        {#each roles.data as r (r.id)}
          <button type="button" class="perm-ligne" aria-current={choisi === r.id} onclick={() => (choisi = r.id)}>
            <b>{r.name}</b>
            <span class="acc-puces">
              {#if r.system}<span class="v-tag">système</span>{/if}
              <span class="v-mute">{r.rules_count} règle{r.rules_count > 1 ? 's' : ''} · {r.system === 'tout-le-monde' ? 'tous les comptes' : `${r.accounts_count} compte${r.accounts_count > 1 ? 's' : ''}`}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}
    <details class="perm-nouveau">
      <summary>Nouveau rôle</summary>
      <form class="perm-form" onsubmit={creer}>
        <div class="plbls-field"><label class="plbls-field__label" for="acc-role-nom">Nom</label><input class="plbls-input" id="acc-role-nom" bind:value={nom} maxlength="80" required /></div>
        <button class="plbls-button">Créer</button>
      </form>
    </details>
  </section>
  {#if choisi}
    {#key choisi}<RoleEditeur id={choisi} {catalogue} onChange={roles.reload} onDelete={() => { choisi = null; roles.reload() }} />{/key}
  {:else}
    <section class="v-card perm-carte perm-vide"><p class="v-note">Un rôle regroupe des permissions de toutes les applications. « Tout le monde » s'applique à chaque compte connecté : c'est là qu'on ouvre ou ferme une fonctionnalité pour tous.</p></section>
  {/if}
</div>
