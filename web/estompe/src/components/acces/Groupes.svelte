<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import Attente from './Attente.svelte'
  import { charger, ecrire, enc, executer, nomCompte, quand, versIso, type Catalogue, type GroupeDetail, type GroupeResume } from '../../lib/acces.svelte'
  import ChoixCompte from './ChoixCompte.svelte'
  import FormLiaison from './FormLiaison.svelte'
  import FormRegle from './FormRegle.svelte'
  import ListeLiaisons from './ListeLiaisons.svelte'
  import ListeRegles from './ListeRegles.svelte'

  let { catalogue }: { catalogue: Catalogue } = $props()
  const groupes = charger<GroupeResume[]>(() => '/groups')
  let choisi = $state<string | null>(null)
  const detail = charger<GroupeDetail>(() => (choisi ? `/groups/${choisi}` : null))
  let nom = $state('')
  let expire = $state('')
  const recharger = () => { detail.reload(); groupes.reload() }

  const creer = async (e: SubmitEvent) => {
    e.preventDefault()
    const g = await executer(ecrire<GroupeDetail>('POST', '/groups', { name: nom }), 'Groupe créé')
    if (g) { nom = ''; groupes.reload(); choisi = g.id }
  }
  const ajouter = async (sub: string) => {
    if ((await executer(ecrire('PUT', `/groups/${choisi}/members/${enc(sub)}`, { expires_at: versIso(expire) }), 'Membre ajouté')) !== null) { expire = ''; recharger() }
  }
  const retirer = async (sub: string, qui: string) => {
    if (!(await confirmer({ titre: 'Retirer du groupe', message: `Retirer ${qui} du groupe ?`, action: 'Retirer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/groups/${choisi}/members/${enc(sub)}`), 'Membre retiré')) !== null) recharger()
  }
  const supprimer = async (g: GroupeDetail) => {
    if (!(await confirmer({ titre: 'Supprimer le groupe', message: `Supprimer « ${g.name} » ? Ses ${g.members.length} membre(s) perdent les rôles et règles du groupe.`, action: 'Supprimer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/groups/${g.id}`), 'Groupe supprimé')) !== null) { choisi = null; groupes.reload() }
  }
</script>

<div class="perm-cols acc-cols">
  <section class="v-card perm-carte">
    <h2 class="perm-t2">Groupes</h2>
    {#if groupes.data === null}
      <Attente error={groupes.error} retry={groupes.reload} />
    {:else}
      <div class="perm-lignes">
        {#each groupes.data as g (g.id)}
          <button type="button" class="perm-ligne" aria-current={choisi === g.id} onclick={() => (choisi = g.id)}>
            <b>{g.name}</b><span class="v-mute">{g.members_count} membre{g.members_count > 1 ? 's' : ''} · {g.roles_count} rôle{g.roles_count > 1 ? 's' : ''}</span>
          </button>
        {:else}
          <p class="v-note">Aucun groupe. Un groupe reçoit des rôles et des règles une fois pour toutes ses membres.</p>
        {/each}
      </div>
    {/if}
    <details class="perm-nouveau">
      <summary>Nouveau groupe</summary>
      <form class="perm-form" onsubmit={creer}>
        <div class="plbls-field"><label class="plbls-field__label" for="acc-g-nom">Nom</label><input class="plbls-input" id="acc-g-nom" bind:value={nom} maxlength="80" required /></div>
        <button class="plbls-button">Créer</button>
      </form>
    </details>
  </section>

  {#if !choisi}
    <section class="v-card perm-carte perm-vide"><p class="v-note">Choisissez un groupe.</p></section>
  {:else if detail.data === null}
    <section class="v-card perm-carte"><Attente error={detail.error} retry={detail.reload} /></section>
  {:else}
    {@const g = detail.data}
    <section class="v-card perm-carte acc-fiche">
      <div class="perm-tete">
        <h2 class="perm-t">{g.name}</h2>
        <button class="plbls-button plbls-button--secondaire plbls-button--compact" onclick={() => supprimer(g)}>Supprimer</button>
      </div>
      <h3 class="perm-t2">Membres</h3>
      {#if g.members.length === 0}<p class="v-note">Aucun membre.</p>{:else}
        <div class="perm-defile">
          <table class="perm-tb">
            <thead><tr><th scope="col">Compte</th><th scope="col">Expire</th><th scope="col">Ajouté</th><th scope="col"><span class="perm-sr">Actions</span></th></tr></thead>
            <tbody>
              {#each g.members as m (m.account.sub)}
                <tr>
                  <td><b>{nomCompte(m.account)}</b>{#if !m.account.in_keycloak} <span class="v-tag dgr">supprimé de Keycloak</span>{/if}</td>
                  <td class="v-note">{m.expires_at ? quand(m.expires_at) : 'jamais'}</td>
                  <td class="v-note">{quand(m.added_at)}</td>
                  <td class="perm-act"><button class="perm-suppr" onclick={() => retirer(m.account.sub, nomCompte(m.account))}>Retirer</button></td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
      <div class="perm-form perm-form--haut">
        <div class="plbls-field perm-compte"><label class="plbls-field__label" for="acc-g-membre">Ajouter un compte</label><ChoixCompte id="acc-g-membre" onPick={(c) => ajouter(c.sub)} /></div>
        <div class="plbls-field"><label class="plbls-field__label" for="acc-g-exp">Jusqu'au (facultatif)</label><input class="plbls-input" id="acc-g-exp" type="datetime-local" bind:value={expire} /></div>
      </div>
      <h3 class="perm-t2">Rôles du groupe</h3>
      <ListeLiaisons liaisons={g.bindings} onChange={recharger} vide="Aucun rôle." />
      <details class="perm-nouveau"><summary>Donner un rôle au groupe</summary><FormLiaison {catalogue} sujet="group" id={g.id} onDone={recharger} /></details>
      <h3 class="perm-t2">Règles du groupe</h3>
      <ListeRegles regles={g.rules} {catalogue} onChange={recharger} vide="Aucune règle propre au groupe." />
      <details class="perm-nouveau"><summary>Accorder ou refuser une permission au groupe</summary><FormRegle {catalogue} sujet="group" id={g.id} onDone={recharger} /></details>
    </section>
  {/if}
</div>
