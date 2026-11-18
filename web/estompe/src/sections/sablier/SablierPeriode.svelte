<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { libelle, nomFleches, PERIODES, peutAvancer, peutReculer } from './periodes'
  import { sablier } from './sablier.svelte'

  const fleches = $derived(nomFleches(sablier.periode))
</script>

<div class="sab-tete">
  <div class="v-seg sab-onglets" role="group" aria-label="Période">
    {#each PERIODES as p (p.id)}
      <button type="button" aria-pressed={sablier.periode === p.id} onclick={() => sablier.choisir(p.id)}>{p.label}</button>
    {/each}
  </div>
  {#if sablier.periode !== 'all'}
    <div class="sab-periode">
      <button class="v-ib" type="button" aria-label={fleches[0]} disabled={!peutReculer(sablier.periode, sablier.date, sablier.premierJour)} onclick={() => sablier.reculer()}><Icon name="gauche" size={18} /></button>
      <span aria-live="polite">{libelle(sablier.periode, sablier.date)}</span>
      <button class="v-ib" type="button" aria-label={fleches[1]} disabled={!peutAvancer(sablier.periode, sablier.date, sablier.aujourdhui)} onclick={() => sablier.avancer()}><Icon name="droite" size={18} /></button>
    </div>
  {/if}
</div>
