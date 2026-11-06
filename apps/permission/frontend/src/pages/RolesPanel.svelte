<script lang="ts">
  import { api, type AppInfo, type Permission, type Role } from "../api";
  import Confirm from "../lib/Confirm.svelte";
  import Pending from "../lib/Pending.svelte";
  import { load, run } from "../lib/state.svelte";
  import RoleForm from "./RoleForm.svelte";

  let { app, perms, dirty, guard }: { app: AppInfo; perms: Permission[] | null; dirty: { current: boolean }; guard: () => Promise<boolean> } = $props();
  const roles = load<Role[]>(() => `/v1/apps/${app.key}/roles`);
  let sel = $state<string | null>(null);
  let creating = $state(false);
  let confirm: Confirm;
  const base = $derived(`/v1/apps/${app.key}`);
  const role = $derived(roles.data?.find((r) => r.key === sel) ?? null);

  const remove = async (r: Role) => {
    if (!(await confirm.ask(`Supprimer le rôle ${r.key} ? Les comptes qui l'ont le perdent.`))) return;
    run(api("DELETE", `${base}/roles/${r.key}`), "Rôle supprimé").then((ok) => { if (ok) { sel = null; roles.reload(); } });
  };
</script>

<section class="plbls-panel">
  <h3 class="plbls-panel__titre">Rôles</h3>
  {#if roles.data === null}<Pending error={roles.error} retry={roles.reload} />{/if}
  {#if roles.data}
    <ul class="app-list">
      {#each roles.data as r (r.key)}
        <li>
          <button type="button" class="plbls-chip{!creating && r.key === sel ? ' is-active' : ''}" aria-pressed={!creating && r.key === sel} onclick={async () => { if ((creating || r.key !== sel) && (await guard())) { creating = false; sel = r.key; } }} title={r.name}>{r.key} · {r.permissions.length}</button>
        </li>
      {/each}
      <li>
        <button type="button" class="plbls-chip{creating ? ' is-active' : ''}" aria-pressed={creating} onclick={async () => { if (!creating && (await guard())) creating = true; }}>+ Nouveau rôle</button>
      </li>
    </ul>
    {#if perms?.length === 0}<p class="app-sub">Déclare d'abord des permissions pour pouvoir les donner à un rôle.</p>{/if}
    {#if creating && perms}
      <RoleForm
        {perms}
        dirtyRef={dirty}
        onSubmit={(v) => run(api("POST", `${base}/roles`, v), `Rôle ${v.key} créé`).then((ok) => { if (ok) { creating = false; sel = v.key; roles.reload(); } })}
        onCancel={() => { creating = false; }}
      />
    {/if}
    {#if !creating && role && perms}
      {#key role.key}
        <RoleForm
          {role}
          {perms}
          dirtyRef={dirty}
          onSubmit={(v) => run(api("PUT", `${base}/roles/${role.key}`, v), `Rôle ${role.key} enregistré`).then((ok) => ok && roles.reload())}
          onDelete={() => remove(role)}
        />
      {/key}
    {/if}
    {#if !creating && !role}
      <p class="app-sub">{roles.data.length === 0 ? "Aucun rôle pour l'instant." : "Choisis un rôle pour voir et modifier ses permissions."}</p>
    {/if}
  {/if}
  <Confirm bind:this={confirm} />
</section>
