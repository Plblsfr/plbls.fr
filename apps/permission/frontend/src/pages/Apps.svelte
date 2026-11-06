<script lang="ts">
  import { api, type AppInfo } from "../api";
  import Confirm from "../lib/Confirm.svelte";
  import Pending from "../lib/Pending.svelte";
  import { load, run } from "../lib/state.svelte";
  import AppDetail from "./AppDetail.svelte";

  const apps = load<AppInfo[]>(() => "/v1/apps");
  let sel = $state<string | null>(null);
  let key = $state("");
  let name = $state("");
  let confirm: Confirm;
  const dirty = { current: false };
  const guard = async () => !dirty.current || confirm.ask("Abandonner les modifications non enregistrées de ce rôle ?", "Abandonner");
  const selected = $derived(sel ? apps.data?.find((a) => a.key === sel) : undefined);

  const create = () => {
    const k = key;
    run(api("POST", "/v1/apps", { key: k, name }), `Application ${k} créée`).then((ok) => { if (ok) { sel = k; key = ""; name = ""; apps.reload(); } });
  };
  const remove = async (k: string) => {
    if (!(await confirm.ask(`Supprimer l'application ${k}, ses permissions, ses rôles et toutes les attributions associées ?`))) return;
    run(api("DELETE", `/v1/apps/${k}`), `Application ${k} supprimée`).then((ok) => { if (ok) { sel = null; apps.reload(); } });
  };
</script>

<div class="app-cols">
  <section class="plbls-panel">
    <h2 class="plbls-panel__titre">Applications</h2>
    {#if apps.data === null}
      <Pending error={apps.error} retry={apps.reload} />
    {:else if apps.data.length === 0}
      <p class="app-sub">Aucune application. Crée la première ci-dessous.</p>
    {:else}
      <div class="app-rows">
        {#each apps.data as a (a.key)}
          <button type="button" class="app-row" aria-current={a.key === sel} onclick={async () => { if (a.key !== sel && (await guard())) sel = a.key; }}><b>{a.name}</b><span class="app-sub app-mono">{a.key}</span></button>
        {/each}
      </div>
    {/if}
    <details class="app-new" open={apps.data?.length === 0}>
      <summary>Nouvelle application</summary>
      <form onsubmit={(e) => { e.preventDefault(); create(); }} class="app-stack">
        <div class="plbls-field">
          <label class="plbls-field__label" for="app-key">Clé</label>
          <input class="plbls-input" id="app-key" bind:value={key} placeholder="chat" pattern={"[a-z][a-z0-9\\-]{0,39}"} required aria-describedby="app-key-aide" />
          <span class="plbls-field__aide" id="app-key-aide">minuscules, chiffres et tirets</span>
        </div>
        <div class="plbls-field">
          <label class="plbls-field__label" for="app-name">Nom</label>
          <input class="plbls-input" id="app-name" bind:value={name} placeholder="Chat" required />
        </div>
        <button class="plbls-button plbls-button--block">Créer l'application</button>
      </form>
    </details>
  </section>

  {#if selected}
    {#key selected.key}
      <AppDetail app={selected} onDelete={() => remove(selected.key)} {dirty} {guard} />
    {/key}
  {:else}
    <div class="plbls-empty-state">
      <h3>Choisis une application</h3>
      <p>Ses permissions et ses rôles s'affichent ici.</p>
    </div>
  {/if}
  <Confirm bind:this={confirm} />
</div>
