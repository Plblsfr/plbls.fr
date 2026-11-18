<script lang="ts">
  import { hrefs } from '../../lib/route.svelte'
  import { ONGLETS, ongletDe } from './admin'
  import SablierAdminJournal from './SablierAdminJournal.svelte'
  import SablierAdminPersonnes from './SablierAdminPersonnes.svelte'
  import SablierAdminTotaux from './SablierAdminTotaux.svelte'
  import SablierAdminTrophees from './SablierAdminTrophees.svelte'
  import SablierPersonne from './SablierPersonne.svelte'
  import SablierTropheeEdition from './SablierTropheeEdition.svelte'

  let { onglet, detail }: { onglet?: string; detail?: string } = $props()
  const actif = $derived(ongletDe(onglet))
</script>

<div class="film-page sab">
  <div class="sab-page">
    <header class="sab-sec">
      <h1 class="sab-h1"><span class="sab-hl">Administration</span> des stats</h1>
      <nav class="v-seg sab-onglets sab-onglets--liens" aria-label="Administration">
        {#each ONGLETS as o (o.id)}<a href={hrefs.stats('admin', o.id)} aria-current={actif === o.id ? 'page' : undefined}>{o.label}</a>{/each}
      </nav>
    </header>
    {#if actif === 'personnes' && detail}
      {#key detail}<SablierPersonne sub={detail} />{/key}
    {:else if actif === 'personnes'}
      <SablierAdminPersonnes />
    {:else if actif === 'totaux'}
      <SablierAdminTotaux />
    {:else if actif === 'journal'}
      <SablierAdminJournal />
    {:else if detail}
      {#key detail}<SablierTropheeEdition id={detail === 'nouveau' ? null : Number(detail)} />{/key}
    {:else}
      <SablierAdminTrophees />
    {/if}
  </div>
</div>
