<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { libelle, nomFleches, PERIODES, peutAvancer, peutReculer } from './periodes'
  import { sablier, type NavigationPeriode } from './sablier.svelte'

  let { etat = sablier }: { etat?: NavigationPeriode } = $props()
  const fleches = $derived(nomFleches(etat.periode))
</script>

<div class="sab-tete">
  <div class="v-seg sab-onglets" role="group" aria-label="Période">
    {#each PERIODES as p (p.id)}
      <button type="button" aria-pressed={etat.periode === p.id} onclick={() => etat.choisir(p.id)}>{p.label}</button>
    {/each}
  </div>
  {#if etat.periode !== 'all'}
    <div class="sab-periode">
      <button class="v-ib" type="button" aria-label={fleches[0]} disabled={!peutReculer(etat.periode, etat.date, etat.premierJour)} onclick={() => etat.reculer()}><Icon name="gauche" size={18} /></button>
      <span aria-live="polite">{libelle(etat.periode, etat.date)}</span>
      <button class="v-ib" type="button" aria-label={fleches[1]} disabled={!peutAvancer(etat.periode, etat.date, etat.aujourdhui)} onclick={() => etat.avancer()}><Icon name="droite" size={18} /></button>
    </div>
  {/if}
</div>
