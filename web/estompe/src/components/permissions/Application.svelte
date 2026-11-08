<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { api } from '../../lib/api'
  import { charger, enc, executer, RESERVEE, type AppInfo, type Permission } from '../../lib/permissions.svelte'
  import Attente from './Attente.svelte'
  import Roles from './Roles.svelte'

  let { app, onDelete, dirty, guard }: { app: AppInfo; onDelete: () => void; dirty: { current: boolean }; guard: () => Promise<boolean> } = $props()
  const base = $derived(`/apps/${enc(app.key)}`)
  const perms = charger<Permission[]>(() => `${base}/permissions`)
  let pKey = $state('')
  let pDesc = $state('')
  let versionRoles = $state(0)
  let onglet = $state<'roles' | 'perms'>('roles')

  const ajouter = () => executer(api.permissions('POST', `${base}/permissions`, { key: pKey, description: pDesc }), `Permission ${app.key}:${pKey} ajoutée`).then((ok) => { if (ok) { pKey = ''; pDesc = ''; perms.reload() } })
  const retirer = async (k: string) => {
    if (!(await confirmer({ titre: 'Supprimer la permission', message: `Supprimer la permission ${app.key}:${k} ? Elle sera retirée de tous les rôles.`, action: 'Supprimer', destructif: true }))) return
    void executer(api.permissions('DELETE', `${base}/permissions/${enc(k)}`), 'Permission supprimée').then((ok) => { if (ok) { perms.reload(); versionRoles += 1 } })
  }
  const clavier = (e: KeyboardEvent) => {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return
    onglet = onglet === 'roles' ? 'perms' : 'roles'
    document.getElementById(`perm-tab-${onglet}`)?.focus()
  }
</script>

<div class="perm-pile">
  <div class="perm-tete">
    <h2 class="perm-t">{app.name} <span class="v-mono v-mute">{app.key}</span></h2>
    {#if app.key !== RESERVEE}
      <button class="plbls-button plbls-button--discret plbls-button--danger plbls-button--compact" onclick={onDelete}>Supprimer l'application</button>
    {/if}
  </div>
  <div class="v-tabs" role="tablist" aria-label="Contenu de l'application" tabindex="-1" onkeydown={clavier}>
    <button role="tab" id="perm-tab-roles" tabindex={onglet === 'roles' ? 0 : -1} aria-selected={onglet === 'roles'} onclick={() => (onglet = 'roles')}>Rôles</button>
    <button role="tab" id="perm-tab-perms" tabindex={onglet === 'perms' ? 0 : -1} aria-selected={onglet === 'perms'} onclick={() => (onglet = 'perms')}>Permissions</button>
  </div>

  <div class="v-card perm-carte" role="tabpanel" aria-labelledby="perm-tab-perms" hidden={onglet !== 'perms'}>
    <h3 class="perm-t2">Permissions</h3>
    {#if perms.data === null}<Attente error={perms.error} retry={perms.reload} />{/if}
    {#if perms.data && perms.data.length > 0}
      <div class="perm-defile">
        <table class="perm-tb">
          <thead><tr><th scope="col">Permission</th><th scope="col">Description</th><th scope="col"><span class="perm-sr">Actions</span></th></tr></thead>
          <tbody>
            {#each perms.data as p (p.key)}
              <tr>
                <td class="v-mono">{app.key}:{p.key}</td>
                <td class="v-note">{p.description}</td>
                <td class="perm-act"><button class="plbls-button plbls-button--lien plbls-button--danger plbls-button--compact" onclick={() => retirer(p.key)}>Supprimer</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    {#if perms.data?.length === 0}<p class="v-note">Aucune permission déclarée.</p>{/if}
    <form class="perm-form" onsubmit={(e) => { e.preventDefault(); void ajouter() }}>
      <div class="plbls-field">
        <label class="plbls-field__label" for="perm-p-key">Permission</label>
        <input class="plbls-input" id="perm-p-key" bind:value={pKey} placeholder="message.delete" pattern={'[a-z][a-z0-9_.\\-]{0,59}'} required />
      </div>
      <div class="plbls-field">
        <label class="plbls-field__label" for="perm-p-desc">Description</label>
        <input class="plbls-input" id="perm-p-desc" bind:value={pDesc} placeholder="Supprimer le message d'un autre" />
      </div>
      <button class="plbls-button plbls-button--contour">Ajouter</button>
    </form>
  </div>

  <div role="tabpanel" aria-labelledby="perm-tab-roles" hidden={onglet !== 'roles'}>
    {#key versionRoles}
      <Roles {app} perms={perms.data} {dirty} {guard} />
    {/key}
  </div>
</div>
