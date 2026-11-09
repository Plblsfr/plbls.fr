<script lang="ts">
  import Attente from '../permissions/Attente.svelte'
  import { charger, type Catalogue } from '../../lib/acces.svelte'
  import CatalogueVue from './CatalogueVue.svelte'
  import Comptes from './Comptes.svelte'
  import Groupes from './Groupes.svelte'
  import JournalAcces from './JournalAcces.svelte'
  import Roles from './Roles.svelte'

  const ONGLETS = { comptes: 'Comptes', roles: 'Rôles', groupes: 'Groupes', catalogue: 'Catalogue', journal: 'Journal' } as const
  type Onglet = keyof typeof ONGLETS
  let onglet = $state<Onglet>('comptes')
  const ordre = Object.keys(ONGLETS) as Onglet[]
  const catalogue = charger<Catalogue>(() => '/catalog')
  const clavier = (e: KeyboardEvent) => {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return
    const i = (ordre.indexOf(onglet) + (e.key === 'ArrowRight' ? 1 : ordre.length - 1)) % ordre.length
    onglet = ordre[i]
    document.getElementById(`acc-onglet-${onglet}`)?.focus()
  }
</script>

<div class="perm acc">
  <div class="v-tabs" role="tablist" aria-label="Gestion des accès" tabindex="-1" onkeydown={clavier}>
    {#each ordre as o (o)}
      <button role="tab" id="acc-onglet-{o}" aria-selected={onglet === o} aria-controls="acc-panneau" tabindex={onglet === o ? 0 : -1} onclick={() => (onglet = o)}>{ONGLETS[o]}</button>
    {/each}
  </div>
  <div id="acc-panneau" role="tabpanel" aria-labelledby="acc-onglet-{onglet}">
    {#if catalogue.data === null}
      <Attente error={catalogue.error} retry={catalogue.reload} />
    {:else if onglet === 'comptes'}<Comptes catalogue={catalogue.data} />
    {:else if onglet === 'roles'}<Roles catalogue={catalogue.data} />
    {:else if onglet === 'groupes'}<Groupes catalogue={catalogue.data} />
    {:else if onglet === 'catalogue'}<CatalogueVue catalogue={catalogue.data} onChange={catalogue.reload} />
    {:else}<JournalAcces catalogue={catalogue.data} />{/if}
  </div>
</div>
