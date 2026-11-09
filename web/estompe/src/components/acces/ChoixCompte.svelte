<script lang="ts">
  import { notify } from '../../lib/ui'
  import { enc, lire, nomCompte, type Compte, type PageComptes } from '../../lib/acces.svelte'

  let { id, onPick, placeholder = "e-mail, nom ou sub" }: { id: string; onPick: (c: Compte) => void; placeholder?: string } = $props()
  let q = $state('')
  let trouves = $state.raw<PageComptes | null>(null)

  $effect(() => {
    const t = q.trim()
    trouves = null
    if (t.length < 2) return
    let perime = false
    const h = setTimeout(() => lire<PageComptes>(`/accounts?q=${enc(t)}&limit=8`).then((p) => { if (!perime) trouves = p }, (e) => { if (!perime) notify.error(e, 'Recherche de comptes impossible') }), 250)
    return () => { perime = true; clearTimeout(h) }
  })
</script>

<input class="plbls-input" {id} type="search" bind:value={q} {placeholder} autocomplete="off" />
{#if trouves}
  {#if trouves.accounts.length === 0}
    <p class="v-note">Aucun compte, ni dans l'annuaire ni dans Keycloak.</p>
  {:else}
    <div class="perm-lignes">
      {#each trouves.accounts as c (c.sub)}
        <button type="button" class="perm-ligne" onclick={() => { onPick(c); q = '' }}>
          <b>{nomCompte(c)}</b><span class="v-mono v-mute">{c.display_name && c.email ? `${c.display_name} · ` : ''}{c.sub}</span>
        </button>
      {/each}
    </div>
  {/if}
{/if}
