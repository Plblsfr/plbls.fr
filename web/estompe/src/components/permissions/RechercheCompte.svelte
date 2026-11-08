<script lang="ts">
  import { notify } from '../../lib/ui'
  import { enc, lire, type FoundUsers } from '../../lib/permissions.svelte'

  let { onPick }: { onPick: (id: string, email: string) => void } = $props()
  let email = $state('')
  let trouves = $state.raw<FoundUsers | null>(null)
  let suite = $state(false)
  const chemin = (q: string, first: number) => `/users?email=${enc(q)}&first=${first}`
  const echec = (e: unknown) => notify.error(e, 'Recherche de comptes impossible')

  $effect(() => {
    const q = email.trim()
    trouves = null
    if (!q) return
    let perime = false
    const t = setTimeout(() => lire<FoundUsers>(chemin(q, 0)).then((f) => { if (!perime) trouves = f }, (e) => { if (!perime) echec(e) }), 300)
    return () => { perime = true; clearTimeout(t) }
  })

  const plus = () => {
    const avant = trouves
    if (!avant || avant.next === null) return
    suite = true
    lire<FoundUsers>(chemin(email.trim(), avant.next)).then((f) => { trouves = { ...f, users: [...avant.users, ...f.users] } }, echec).finally(() => { suite = false })
  }
</script>

<input class="plbls-input" id="perm-a-user" type="search" bind:value={email} placeholder="début de l'e-mail" autocomplete="off" />
{#if trouves}
  {#if trouves.users.length === 0 && trouves.next === null}
    <p class="v-note">Aucun compte.</p>
  {:else}
    <div class="perm-lignes">
      {#each trouves.users as u (u.id)}
        <button type="button" class="perm-ligne" onclick={() => { onPick(u.id, u.email); email = '' }}><b>{u.email}</b><span class="v-mono v-mute">{u.id}</span></button>
      {/each}
      {#if trouves.next !== null}<button type="button" class="plbls-button plbls-button--lien plbls-button--compact" disabled={suite} onclick={plus}>Plus de résultats</button>{/if}
    </div>
  {/if}
{/if}
