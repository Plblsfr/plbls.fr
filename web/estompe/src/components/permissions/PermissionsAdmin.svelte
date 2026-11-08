<script lang="ts">
  import Applications from './Applications.svelte'
  import Attributions from './Attributions.svelte'
  import Journal from './Journal.svelte'

  const ONGLETS = { apps: 'Applications', attributions: 'Attributions', journal: 'Journal' } as const
  type Onglet = keyof typeof ONGLETS
  let onglet = $state<Onglet>('apps')
  const ordre = Object.keys(ONGLETS) as Onglet[]
  const clavier = (e: KeyboardEvent) => {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return
    const i = (ordre.indexOf(onglet) + (e.key === 'ArrowRight' ? 1 : ordre.length - 1)) % ordre.length
    onglet = ordre[i]
    document.getElementById(`perm-onglet-${onglet}`)?.focus()
  }
</script>

<div class="perm">
  <div class="v-tabs" role="tablist" aria-label="Gestion des permissions" tabindex="-1" onkeydown={clavier}>
    {#each ordre as o (o)}
      <button role="tab" id="perm-onglet-{o}" aria-selected={onglet === o} aria-controls="perm-panneau" tabindex={onglet === o ? 0 : -1} onclick={() => (onglet = o)}>{ONGLETS[o]}</button>
    {/each}
  </div>
  <div id="perm-panneau" role="tabpanel" aria-labelledby="perm-onglet-{onglet}">
    {#if onglet === 'apps'}<Applications />{:else if onglet === 'attributions'}<Attributions />{:else}<Journal />{/if}
  </div>
</div>
