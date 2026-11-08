<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { api } from '../../lib/api'
  import { charger, enc, executer, type AppInfo, type Permission, type PermRole } from '../../lib/permissions.svelte'
  import Attente from './Attente.svelte'
  import RoleForm from './RoleForm.svelte'

  let { app, perms, dirty, guard }: { app: AppInfo; perms: Permission[] | null; dirty: { current: boolean }; guard: () => Promise<boolean> } = $props()
  const base = $derived(`/apps/${enc(app.key)}`)
  const roles = charger<PermRole[]>(() => `${base}/roles`)
  let sel = $state<string | null>(null)
  let creation = $state(false)
  const role = $derived(roles.data?.find((r) => r.key === sel) ?? null)

  const supprimer = async (r: PermRole) => {
    if (!(await confirmer({ titre: 'Supprimer le rôle', message: `Supprimer le rôle ${r.key} ? Les comptes qui l'ont le perdent.`, action: 'Supprimer', destructif: true }))) return
    void executer(api.permissions('DELETE', `${base}/roles/${enc(r.key)}`), 'Rôle supprimé').then((ok) => { if (ok) { sel = null; roles.reload() } })
  }
</script>

<section class="v-card perm-carte">
  <h3 class="perm-t2">Rôles</h3>
  {#if roles.data === null}<Attente error={roles.error} retry={roles.reload} />{/if}
  {#if roles.data}
    <ul class="perm-puces">
      {#each roles.data as r (r.key)}
        <li><button class="v-chip" aria-pressed={!creation && r.key === sel} title={r.name} onclick={async () => { if ((creation || r.key !== sel) && (await guard())) { creation = false; sel = r.key } }}>{r.key} · {r.permissions.length}</button></li>
      {/each}
      <li><button class="v-chip line" aria-pressed={creation} onclick={async () => { if (!creation && (await guard())) creation = true }}>+ Nouveau rôle</button></li>
    </ul>
    {#if perms?.length === 0}<p class="v-note">Déclare d'abord des permissions pour pouvoir les donner à un rôle.</p>{/if}
    {#if creation && perms}
      <RoleForm {perms} dirtyRef={dirty}
        onSubmit={(v) => executer(api.permissions('POST', `${base}/roles`, v), `Rôle ${v.key} créé`).then((ok) => { if (ok) { creation = false; sel = v.key; roles.reload() } })}
        onCancel={() => (creation = false)} />
    {/if}
    {#if !creation && role && perms}
      {#key role.key}
        <RoleForm {role} {perms} dirtyRef={dirty}
          onSubmit={(v) => executer(api.permissions('PUT', `${base}/roles/${enc(role.key)}`, v), `Rôle ${role.key} enregistré`).then((ok) => ok && roles.reload())}
          onDelete={() => supprimer(role)} />
      {/key}
    {/if}
    {#if !creation && !role}
      <p class="v-note">{roles.data.length === 0 ? "Aucun rôle pour l'instant." : 'Choisis un rôle pour voir et modifier ses permissions.'}</p>
    {/if}
  {/if}
</section>
