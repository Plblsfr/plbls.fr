<script lang="ts">
  import { api, type FoundUsers } from "../api";
  import { fail } from "../lib/state.svelte";

  let { onPick }: { onPick: (id: string, email: string) => void } = $props();
  let email = $state("");
  let found = $state.raw<FoundUsers | null>(null);
  let more = $state(false);
  const path = (q: string, first: number) => `/v1/users?email=${encodeURIComponent(q)}&first=${first}`;

  $effect(() => {
    const q = email.trim();
    found = null;
    if (!q) return;
    let stale = false;
    const t = setTimeout(() => api<FoundUsers>("GET", path(q, 0)).then((f) => { if (!stale) found = f; }, (e) => { if (!stale) fail(e); }), 300);
    return () => { stale = true; clearTimeout(t); };
  });

  const loadMore = () => {
    const prev = found;
    if (!prev || prev.next === null) return;
    more = true;
    api<FoundUsers>("GET", path(email.trim(), prev.next)).then((f) => { found = { ...f, users: [...prev.users, ...f.users] }; }, fail).finally(() => { more = false; });
  };
</script>

<input class="plbls-input" id="a-user" type="search" bind:value={email} placeholder="début de l'e-mail" autocomplete="off" />
{#if found}
  {#if found.users.length === 0 && found.next === null}
    <p class="app-sub">Aucun compte.</p>
  {:else}
    <div class="app-rows">
      {#each found.users as u (u.id)}
        <button type="button" class="app-row" onclick={() => { onPick(u.id, u.email); email = ""; }}><b>{u.email}</b><span class="app-sub app-mono">{u.id}</span></button>
      {/each}
      {#if found.next !== null}<button type="button" class="plbls-button plbls-button--lien plbls-button--compact" disabled={more} onclick={loadMore}>Plus de résultats</button>{/if}
    </div>
  {/if}
{/if}
