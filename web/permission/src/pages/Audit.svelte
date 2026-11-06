<script lang="ts">
  import type { AuditEntry } from "../api";
  import Pending from "../lib/Pending.svelte";
  import { load } from "../lib/state.svelte";

  const TONE: Record<string, string> = { create: "positif", grant: "positif", update: "accent", delete: "negatif", revoke: "negatif" };
  const rows = load<AuditEntry[]>(() => "/v1/audit?limit=200");
</script>

<section class="plbls-panel">
  <h2 class="plbls-panel__titre">Journal</h2>
  {#if rows.data === null}
    <Pending error={rows.error} retry={rows.reload} />
  {:else if rows.data.length === 0}
    <div class="plbls-empty-state"><h3>Rien à signaler</h3><p>Aucune modification enregistrée.</p></div>
  {:else}
    <div class="app-scroll">
      <table class="plbls-table">
        <thead><tr><th scope="col">Date</th><th scope="col">Acteur</th><th scope="col">Action</th><th scope="col">Cible</th><th scope="col">Détail</th></tr></thead>
        <tbody>
          {#each rows.data as r}
            <tr>
              <td class="app-sub">{new Date(r.at).toLocaleString("fr-FR")}</td><td class="app-mono">{r.actor}</td>
              <td><span class={`plbls-status plbls-status--${TONE[r.action.split(".")[1]] ?? ""}`.trim()}>{r.action}</span></td>
              <td class="app-mono">{r.target}</td><td class="app-mono">{JSON.stringify(r.detail)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>
