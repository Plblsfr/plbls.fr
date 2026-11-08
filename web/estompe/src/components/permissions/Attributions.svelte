<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { api } from '../../lib/api'
  import { app as estompe } from '../../lib/store.svelte'
  import { charger, enc, executer, type AppInfo, type Assignment, type PermRole } from '../../lib/permissions.svelte'
  import Attente from './Attente.svelte'
  import RechercheCompte from './RechercheCompte.svelte'

  let filtre = $state('')
  const lignes = charger<Assignment[]>(() => `/assignments${filtre.trim() ? `?user_id=${enc(filtre.trim())}` : ''}`)
  const apps = charger<AppInfo[]>(() => '/apps')
  let app = $state('')
  const roles = charger<PermRole[]>(() => (app ? `/apps/${enc(app)}/roles` : null))
  let userId = $state('')
  let userEmail = $state('')
  let role = $state('')
  const recherche = $derived(estompe.rights.includes('users'))

  const attribuer = () => executer(api.permissions('POST', '/assignments', { user_id: userId, app, role }), `${app}/${role} attribué`).then((ok) => ok && lignes.reload())
  const choisir = (id: string, email: string) => { userId = id; userEmail = email }
  const retirer = async (a: Assignment) => {
    if (!(await confirmer({ titre: 'Retirer le rôle', message: `Retirer ${a.app}/${a.role} au compte ${a.user_id} ?`, action: 'Retirer', destructif: true }))) return
    void executer(api.permissions('DELETE', `/assignments?user_id=${enc(a.user_id)}&app=${enc(a.app)}&role=${enc(a.role)}`), 'Rôle retiré').then((ok) => ok && lignes.reload())
  }
</script>

<div class="perm-pile">
  <section class="v-card perm-carte">
    <h2 class="perm-t">Attribuer un rôle</h2>
    <form class="perm-form" onsubmit={(e) => { e.preventDefault(); void attribuer() }}>
      <div class="plbls-field perm-compte">
        <label class="plbls-field__label" for="perm-a-user">{recherche ? 'Compte (e-mail)' : 'Compte (sub Keycloak)'}</label>
        {#if recherche}
          {#if userId}<p class="perm-en-ligne"><b>{userEmail}</b><button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => choisir('', '')}>Changer</button></p>
          {:else}<RechercheCompte onPick={choisir} />{/if}
        {:else}
          <input class="plbls-input" id="perm-a-user" bind:value={userId} placeholder="3f8a1c2e-…" required />
        {/if}
      </div>
      <div class="plbls-field">
        <label class="plbls-field__label" for="perm-a-app">Application</label>
        <select class="plbls-select" id="perm-a-app" bind:value={app} onchange={() => (role = '')} required>
          <option value="">Choisir…</option>
          {#each apps.data ?? [] as a (a.key)}<option value={a.key}>{a.key}</option>{/each}
        </select>
      </div>
      <div class="plbls-field">
        <label class="plbls-field__label" for="perm-a-role">Rôle</label>
        <select class="plbls-select" id="perm-a-role" bind:value={role} required disabled={!app}>
          <option value="">Choisir…</option>
          {#each roles.data ?? [] as r (r.key)}<option value={r.key}>{r.key}</option>{/each}
        </select>
      </div>
      <button class="plbls-button" disabled={!userId}>Attribuer</button>
    </form>
  </section>

  <section class="v-card perm-carte">
    <h2 class="perm-t">Attributions</h2>
    <div class="plbls-field">
      <label class="plbls-field__label" for="perm-a-filtre">Filtrer par compte</label>
      <input class="plbls-input" id="perm-a-filtre" bind:value={filtre} placeholder="sub exact" />
    </div>
    {#if lignes.data === null}
      <Attente error={lignes.error} retry={lignes.reload} />
    {:else if lignes.data.length === 0}
      <p class="v-note">{filtre ? "Ce compte n'a aucun rôle." : "Personne n'a encore de rôle."}</p>
    {:else}
      <div class="perm-defile">
        <table class="perm-tb">
          <thead><tr><th scope="col">Compte</th><th scope="col">Application</th><th scope="col">Rôle</th><th scope="col">Par</th><th scope="col">Date</th><th scope="col"><span class="perm-sr">Actions</span></th></tr></thead>
          <tbody>
            {#each lignes.data as a (a.user_id + a.app + a.role)}
              <tr>
                <td class="v-mono">{a.user_id}</td><td>{a.app}</td><td><span class="v-tag rose">{a.role}</span></td>
                <td class="v-mono">{a.granted_by}</td><td class="v-note">{new Date(a.created_at).toLocaleString('fr-FR')}</td>
                <td class="perm-act"><button class="perm-suppr" onclick={() => retirer(a)}>Retirer</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
</div>
