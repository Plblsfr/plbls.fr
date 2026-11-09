<script lang="ts">
  import { notify } from '../../lib/ui'
  import { enc, lire, type Ressource } from '../../lib/acces.svelte'

  let { id, type, onPick }: { id: string; type: string; onPick: (r: Ressource) => void } = $props()
  let q = $state('')
  let trouves = $state.raw<Ressource[] | null>(null)
  const chemin = $derived(`/resources/${type.split(':').map(enc).join('/')}`)

  $effect(() => {
    const t = q.trim()
    const c = chemin
    trouves = null
    let perime = false
    const h = setTimeout(() => lire<Ressource[]>(`${c}${t ? `?q=${enc(t)}` : ''}`).then((r) => { if (!perime) trouves = r }, (e) => { if (!perime) notify.error(e, 'Liste des ressources indisponible') }), 250)
    return () => { perime = true; clearTimeout(h) }
  })
</script>

<input class="plbls-input" {id} type="search" bind:value={q} placeholder="nom ou identifiant" autocomplete="off" />
{#if trouves}
  <div class="perm-lignes acc-choix">
    {#each trouves as r (r.id)}
      <button type="button" class="perm-ligne" onclick={() => { onPick(r); q = '' }}><b>{r.name}</b><span class="v-mono v-mute">{r.id}</span></button>
    {:else}
      <p class="v-note">Aucune ressource.</p>
    {/each}
  </div>
{/if}
