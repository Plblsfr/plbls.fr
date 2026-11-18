<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import type { Series } from './sablier'
  import { cartesSeries } from './stats'

  let { series }: { series: Series } = $props()
  const liste = $derived(cartesSeries(series))
</script>

{#if liste.length}
  <section class="sab-sec" aria-label="Séries de jours">
    <div class="sab-tete"><h2 class="sab-h2">Séries de jours</h2><p class="sab-note">Une journée compte dès une lecture, une écoute, un message ou un passage en vocal.</p></div>
    <div class="sab-g sab-g{Math.min(4, Math.max(2, liste.length))}">
      {#each liste as s (s.cle)}
        <div class="sab-card" class:sab-card--rose={s.cle === 'global' && s.etat === 'en-cours'} class:sab-card--creux={s.etat === 'arretee'} data-serie={s.cle}>
          <div class="sab-card-tete"><Icon name={s.icone} size={18} />{s.titre}</div>
          {#if s.etat === 'en-cours'}
            <p class="sab-num">{s.nombre} <small>{s.unite}</small></p>
            <p class="sab-note">{s.record}</p>
          {:else}
            <p class="sab-note">Pas de série en cours. {s.record}.</p>
          {/if}
        </div>
      {/each}
    </div>
  </section>
{/if}
