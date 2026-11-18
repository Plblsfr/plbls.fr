<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { libelleRecap } from './periodes'
  import { prochainRecap, rangerRecaps } from './recap'
  import SablierErreur from './SablierErreur.svelte'
  import { sablier } from './sablier.svelte'

  const APERCU = 6
  let anciens = $state(false)
  onMount(() => { void sablier.chargerRecaps() })
  const rangement = $derived(sablier.recaps ? rangerRecaps(sablier.recaps) : null)
  const mois = $derived(rangement ? (anciens ? rangement.mois : rangement.mois.slice(0, APERCU)) : [])
</script>

<div class="film-page sab">
  <div class="sab-page">
    <header class="sab-sec">
      <h1 class="sab-h1">Tes <span class="sab-hl">récaps</span></h1>
      <p class="sab-note">Un récap par mois et par année terminés, figé le 1er du mois suivant. Le prochain arrive le {prochainRecap(sablier.aujourdhui)}.</p>
    </header>
    {#if sablier.erreurRecaps && !rangement}
      <SablierErreur message={sablier.erreurRecaps} onretry={() => void sablier.chargerRecaps()} />
    {:else if !rangement}
      <div class="sab-attente"><Loader text="Ouverture des récaps…" /></div>
    {:else if !sablier.recaps?.length}
      <div class="sab-vide">
        <Icon name="histoire" size={44} />
        <h2 class="sab-h2">Pas encore de récap</h2>
        <p>Le premier arrive le {prochainRecap(sablier.aujourdhui)}, si le mois a eu assez d’activité.</p>
      </div>
    {:else}
      {#if rangement.nouveau}
        {@const n = rangement.nouveau}
        <div class="sab-card sab-card--rose">
          <div class="sab-tete" style:align-items="center">
            <div class="sab-col"><span class="sab-etiquette sab-etiquette--claire" style:align-self="flex-start">Nouveau</span><h2 class="sab-h2">{libelleRecap(n.kind, n.period)}</h2></div>
            <a class="plbls-button" href={hrefs.stats('recaps', n.id)} aria-label="Regarder le récap {libelleRecap(n.kind, n.period)}"><Icon name="lecture" size={16} />Regarder</a>
          </div>
        </div>
      {/if}
      {#if rangement.mois.length}
        <section class="sab-sec" aria-label="Mois">
          <h2 class="sab-h2">Mois</h2>
          <ul class="sab-g sab-g3">
            {#each mois as c (c.periode)}
              {#if c.recap}
                <li class="sab-card"><h3 class="sab-h3">{c.label}</h3><div class="sab-ligne"><a class="plbls-button plbls-button--contour plbls-button--compact" href={hrefs.stats('recaps', c.recap.id)} aria-label="Regarder le récap {c.label}">Regarder</a></div></li>
              {:else}
                <li class="sab-card sab-card--creux"><h3 class="sab-h3">{c.label}</h3><p class="sab-note">Pas de récap : trop peu d’activité ce mois-là.</p></li>
              {/if}
            {/each}
          </ul>
          {#if rangement.mois.length > APERCU && !anciens}
            <div class="sab-ligne"><button class="plbls-button plbls-button--contour plbls-button--compact" type="button" onclick={() => (anciens = true)}>Mois plus anciens</button></div>
          {/if}
        </section>
      {/if}
      {#if rangement.annees.length}
        <section class="sab-sec" aria-label="Années">
          <h2 class="sab-h2">Années</h2>
          <ul class="sab-g sab-g3">
            {#each rangement.annees as c (c.periode)}
              {#if c.recap}<li class="sab-card"><h3 class="sab-h3">{c.label}</h3><div class="sab-ligne"><a class="plbls-button plbls-button--contour plbls-button--compact" href={hrefs.stats('recaps', c.recap.id)} aria-label="Regarder le récap {c.label}">Regarder</a></div></li>{/if}
            {/each}
          </ul>
        </section>
      {/if}
    {/if}
  </div>
</div>
