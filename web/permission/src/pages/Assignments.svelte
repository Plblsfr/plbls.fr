<script lang="ts">
  import { api, type AppInfo, type Assignment, type Permissions, type Role } from "../api";
  import Confirm from "../lib/Confirm.svelte";
  import Pending from "../lib/Pending.svelte";
  import { load, run } from "../lib/state.svelte";
  import UserSearch from "./UserSearch.svelte";

  let filter = $state("");
  const rows = load<Assignment[]>(() => `/v1/assignments${filter ? `?user_id=${encodeURIComponent(filter)}` : ""}`);
  const apps = load<AppInfo[]>(() => "/v1/apps");
  let app = $state("");
  const roles = load<Role[]>(() => (app ? `/v1/apps/${app}/roles` : null));
  let userId = $state("");
  let userEmail = $state("");
  const me = load<Permissions>(() => "/v1/me/permissions");
  const canSearch = $derived(!!me.data?.permissions.includes("permission:users.read"));
  let role = $state("");
  let confirm: Confirm;

  const grant = () => run(api("POST", "/v1/assignments", { user_id: userId, app, role }), `${app}/${role} attribué`).then((ok) => ok && rows.reload());
  const pick = (id: string, email: string) => { userId = id; userEmail = email; };
  const revoke = async (a: Assignment) => {
    if (!(await confirm.ask(`Retirer ${a.app}/${a.role} au compte ${a.user_id} ?`, "Retirer"))) return;
    run(api("DELETE", `/v1/assignments?user_id=${encodeURIComponent(a.user_id)}&app=${a.app}&role=${a.role}`), "Rôle retiré").then((ok) => ok && rows.reload());
  };
</script>

<div class="app-stack">
  <section class="plbls-panel">
    <h2 class="plbls-panel__titre">Attribuer un rôle</h2>
    <form class="app-form" onsubmit={(e) => { e.preventDefault(); grant(); }}>
      <div class="plbls-field" style="flex-basis: 300px;">
        <label class="plbls-field__label" for="a-user">{canSearch ? "Compte (e-mail)" : "Compte (sub Keycloak)"}</label>
        {#if canSearch}
          {#if userId}<p class="app-inline"><b>{userEmail}</b><button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => pick("", "")}>Changer</button></p>{/if}
          {#if !userId}<UserSearch onPick={pick} />{/if}
        {:else}
          <input class="plbls-input" id="a-user" bind:value={userId} placeholder="3f8a1c2e-…" required />
        {/if}
      </div>
      <div class="plbls-field">
        <label class="plbls-field__label" for="a-app">Application</label>
        <select class="plbls-select" id="a-app" bind:value={app} onchange={() => { role = ""; }} required>
          <option value="">Choisir…</option>
          {#each apps.data ?? [] as a (a.key)}<option value={a.key}>{a.key}</option>{/each}
        </select>
      </div>
      <div class="plbls-field">
        <label class="plbls-field__label" for="a-role">Rôle</label>
        <select class="plbls-select" id="a-role" bind:value={role} required disabled={!app}>
          <option value="">Choisir…</option>
          {#each roles.data ?? [] as r (r.key)}<option value={r.key}>{r.key}</option>{/each}
        </select>
      </div>
      <button class="plbls-button" disabled={!userId}>Attribuer</button>
    </form>
  </section>

  <section class="plbls-panel">
    <h2 class="plbls-panel__titre">Attributions</h2>
    <div class="plbls-field">
      <label class="plbls-field__label" for="a-filter">Filtrer par compte</label>
      <input class="plbls-input" id="a-filter" bind:value={filter} placeholder="sub exact" />
    </div>
    {#if rows.data === null}
      <Pending error={rows.error} retry={rows.reload} />
    {:else if rows.data.length === 0}
      <div class="plbls-empty-state"><h3>Aucune attribution</h3><p>{filter ? "Ce compte n'a aucun rôle." : "Personne n'a encore de rôle."}</p></div>
    {:else}
      <div class="app-scroll">
        <table class="plbls-table">
          <thead><tr><th scope="col">Compte</th><th scope="col">Application</th><th scope="col">Rôle</th><th scope="col">Par</th><th scope="col">Date</th><th scope="col"><span class="plbls-sr-only">Actions</span></th></tr></thead>
          <tbody>
            {#each rows.data as a (a.user_id + a.app + a.role)}
              <tr>
                <td class="app-mono">{a.user_id}</td><td>{a.app}</td><td><span class="plbls-status plbls-status--accent">{a.role}</span></td>
                <td class="app-mono">{a.granted_by}</td><td class="app-sub">{new Date(a.created_at).toLocaleString("fr-FR")}</td>
                <td><button type="button" class="plbls-button plbls-button--lien plbls-button--danger plbls-button--compact" onclick={() => revoke(a)}>Retirer</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
  <Confirm bind:this={confirm} />
</div>
