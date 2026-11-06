<script lang="ts">
  import { onMount } from "svelte";
  import { api, type AppInfo, type Permission } from "../api";
  import Confirm from "../lib/Confirm.svelte";
  import Pending from "../lib/Pending.svelte";
  import { load, run } from "../lib/state.svelte";
  import RolesPanel from "./RolesPanel.svelte";

  let { app, onDelete, dirty, guard }: { app: AppInfo; onDelete: () => void; dirty: { current: boolean }; guard: () => Promise<boolean> } = $props();
  const perms = load<Permission[]>(() => `/v1/apps/${app.key}/permissions`);
  let pKey = $state("");
  let pDesc = $state("");
  let confirm: Confirm;
  let rolesVersion = $state(0);
  let tab = $state<"roles" | "perms">("roles");
  const base = $derived(`/v1/apps/${app.key}`);
  let top: HTMLDivElement;
  onMount(() => { if (matchMedia("(max-width: 780px)").matches) top.scrollIntoView({ block: "start" }); });

  const addPerm = () => run(api("POST", `${base}/permissions`, { key: pKey, description: pDesc }), `Permission ${app.key}:${pKey} ajoutée`).then((ok) => { if (ok) { pKey = ""; pDesc = ""; perms.reload(); } });
  const delPerm = async (k: string) => {
    if (!(await confirm.ask(`Supprimer la permission ${app.key}:${k} ? Elle sera retirée de tous les rôles.`))) return;
    run(api("DELETE", `${base}/permissions/${k}`), "Permission supprimée").then((ok) => { if (ok) { perms.reload(); rolesVersion += 1; } });
  };
  const onTabKey = (e: KeyboardEvent) => {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    const t = tab === "roles" ? "perms" : "roles";
    tab = t;
    document.getElementById(`tab-${t}`)?.focus();
  };
</script>

<div class="app-stack" bind:this={top}>
  <div class="app-head">
    <h2>{app.name} <span class="app-sub app-mono">{app.key}</span></h2>
    {#if app.key !== "permission"}
      <button type="button" class="plbls-button plbls-button--discret plbls-button--danger plbls-button--compact" onclick={onDelete}>Supprimer l'application</button>
    {/if}
  </div>
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div class="plbls-tabs app-tabs-wrap" role="tablist" aria-label="Contenu de l'application" onkeydown={onTabKey}>
    <button type="button" role="tab" id="tab-roles" tabindex={tab === "roles" ? 0 : -1} aria-selected={tab === "roles"} class="plbls-tabs__onglet" onclick={() => { tab = "roles"; }}>Rôles</button>
    <button type="button" role="tab" id="tab-perms" tabindex={tab === "perms" ? 0 : -1} aria-selected={tab === "perms"} class="plbls-tabs__onglet" onclick={() => { tab = "perms"; }}>Permissions</button>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
  <section class="plbls-panel" role="tabpanel" aria-labelledby="tab-perms" hidden={tab !== "perms"}>
    <h3 class="plbls-panel__titre">Permissions</h3>
    {#if perms.data === null}<Pending error={perms.error} retry={perms.reload} />{/if}
    {#if perms.data && perms.data.length > 0}
      <div class="app-scroll">
        <table class="plbls-table">
          <thead><tr><th scope="col">Permission</th><th scope="col">Description</th><th scope="col"><span class="plbls-sr-only">Actions</span></th></tr></thead>
          <tbody>
            {#each perms.data as p (p.key)}
              <tr>
                <td class="app-mono">{app.key}:{p.key}</td>
                <td class="app-sub">{p.description}</td>
                <td><button type="button" class="plbls-button plbls-button--lien plbls-button--danger plbls-button--compact" onclick={() => delPerm(p.key)}>Supprimer</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    {#if perms.data?.length === 0}<p class="app-sub">Aucune permission déclarée.</p>{/if}
    <form class="app-form" onsubmit={(e) => { e.preventDefault(); addPerm(); }}>
      <div class="plbls-field">
        <label class="plbls-field__label" for="p-key">Permission</label>
        <input class="plbls-input" id="p-key" bind:value={pKey} placeholder="message.delete" required />
      </div>
      <div class="plbls-field">
        <label class="plbls-field__label" for="p-desc">Description</label>
        <input class="plbls-input" id="p-desc" bind:value={pDesc} placeholder="Supprimer le message d'un autre" />
      </div>
      <button class="plbls-button plbls-button--contour">Ajouter</button>
    </form>
  </section>

  <div role="tabpanel" aria-labelledby="tab-roles" hidden={tab !== "roles"}>
    {#key rolesVersion}
      <RolesPanel {app} perms={perms.data} {dirty} {guard} />
    {/key}
  </div>
  <Confirm bind:this={confirm} />
</div>
