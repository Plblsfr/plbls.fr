<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { jourSansAnnee, pluriel } from './formats'
  import type { JourDuree, Series } from './sablier'
  import { cartesSeries, quatreSemaines } from './stats'

  let { series, activite, aujourdhui }: { series: Series; activite: JourDuree[] | null; aujourdhui: string } = $props()
  const REGLE = 'Une journée compte dès une lecture, une écoute, un message ou un passage en vocal.'
  const liste = $derived(cartesSeries(series))
  const globale = $derived(liste.find((s) => s.cle === 'global'))
  const autres = $derived(liste.filter((s) => s.cle !== 'global' && s.etat === 'en-cours'))
  const jours = $derived(activite ? quatreSemaines(activite, aujourdhui, series.global.current) : [])
  const actifs = $derived(jours.filter((j) => j.etat === 'serie' || j.etat === 'actif').length)
  const ecoules = $derived(jours.filter((j) => j.etat !== 'futur').length)
</script>

{#if liste.length}
  <section class="sab-serie" aria-label="Série de jours">
    <div class="sab-col sab-grow sab-serie-texte">
      <div class="sab-card-tete"><Icon name="flamme" size={18} />Série en cours, toutes activités</div>
      {#if globale?.etat === 'en-cours'}
        <p class="sab-serie-num" data-serie="global">{globale.nombre} <small>{globale.unite}</small></p>
        <p class="sab-note">{globale.record}. {REGLE}</p>
      {:else}
        <p class="sab-h2" data-serie="global">Pas de série en cours</p>
        <p class="sab-note">{globale ? `${globale.record}. ` : ''}{REGLE}</p>
      {/if}
      {#if autres.length}
        <ul class="sab-ligne">
          {#each autres as s (s.cle)}<li class="sab-etiquette" data-serie={s.cle}><Icon name={s.icone} size={14} />{s.titre} · {s.nombre} {s.unite}</li>{/each}
        </ul>
      {/if}
    </div>
    {#if jours.length}
      <div class="sab-col sab-serie-cal">
        <p class="sab-h3">Quatre dernières semaines</p>
        <div class="sab-cal sab-cal--tete" aria-hidden="true"><span>L</span><span>M</span><span>M</span><span>J</span><span>V</span><span>S</span><span>D</span></div>
        <div class="sab-cal" role="img" aria-label="Du {jourSansAnnee(jours[0].jour)} au {jourSansAnnee(aujourdhui)} : {pluriel(actifs, 'jour actif', 'jours actifs')} sur {ecoules}">
          {#each jours as j (j.jour)}<span data-etat={j.etat} data-auj={j.aujourdhui ? '' : undefined}>{j.numero}</span>{/each}
        </div>
      </div>
    {/if}
  </section>
{/if}
