<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { app } from '../../lib/store.svelte'
  import { pluriel } from './formats'
  import { intitule, titre } from './periodes'
  import SablierAnnonce from './SablierAnnonce.svelte'
  import SablierCarte from './SablierCarte.svelte'
  import SablierErreur from './SablierErreur.svelte'
  import SablierPeriode from './SablierPeriode.svelte'
  import SablierSeries from './SablierSeries.svelte'
  import SablierTrophees from './SablierTrophees.svelte'
  import { sablier } from './sablier.svelte'
  import { aucuneActivite, cartes, vueTrophees, type NomSalon } from './stats'

  onMount(() => { void sablier.ouvrir() })

  const nomSalon: NomSalon = (serveur, salon) => app.details[serveur]?.channels.find((c) => c.id === salon)?.name ?? null
  const pret = $derived(sablier.charge && !!sablier.stats && !!sablier.trophees && !!sablier.series)
  const vide = $derived(aucuneActivite(sablier.stats, sablier.trophees))
  const t = $derived(titre(sablier.periode, sablier.date))
  const liste = $derived(sablier.stats ? cartes(sablier.stats, nomSalon, sablier.aujourdhui) : [])
  const aDebloquer = $derived(sablier.trophees ? vueTrophees(sablier.trophees, 'all', sablier.date).total : 0)
</script>

<div class="film-page sab">
  <div class="sab-page">
    {#if !pret && sablier.erreur}
      <h1 class="sab-h1">Tes <span class="sab-hl">stats</span></h1>
      <SablierErreur message={sablier.erreur} onretry={() => void sablier.ouvrir()} />
    {:else if !pret}
      <div class="sab-attente"><Loader text="Ouverture des stats…" /></div>
    {:else if vide}
      <h1 class="sab-h1">Tes <span class="sab-hl">stats</span></h1>
      <div class="sab-vide">
        <Icon name="sablier" size={44} />
        <h2 class="sab-h2">Rien à raconter pour l’instant</h2>
        <p>Tes stats se remplissent dès que tu regardes un titre, écoutes un morceau, envoies un message ou passes en vocal. Elles restent privées tant que tu n’en décides pas autrement.</p>
        {#if app.movies || app.musique}
          <div class="sab-ligne" style:justify-content="center">
            {#if app.movies}<a class="plbls-button" href={hrefs.films()}>Ouvrir Entracte</a>{/if}
            {#if app.musique}<a class="plbls-button plbls-button--contour" href={hrefs.musique()}>Ouvrir Sillon</a>{/if}
          </div>
        {/if}
      </div>
      {#if aDebloquer > 0}
        <div class="sab-tro sab-tro--cache">
          <span class="sab-med sab-med--cache"><Icon name="trophee" size={26} /></span>
          <div class="sab-col"><p class="sab-h3">{pluriel(aDebloquer, 'trophée', 'trophées')} à débloquer</p><p class="sab-note">Le premier arrive dès ta première journée d’activité.</p></div>
        </div>
      {/if}
    {:else if sablier.stats && sablier.trophees && sablier.series}
      <header class="sab-sec">
        <h1 class="sab-h1">{t.avant}<span class="sab-hl">{t.fort}</span></h1>
        <SablierPeriode />
      </header>
      {#if sablier.erreur}
        <div class="plbls-alert plbls-alert--erreur" role="alert">
          <p class="plbls-alert__corps">{sablier.erreur}</p>
          <button class="plbls-button plbls-button--compact plbls-button--contour" type="button" onclick={() => void sablier.chargerStats()}>Réessayer</button>
        </div>
      {/if}
      <SablierSeries series={sablier.series} />
      <SablierTrophees trophees={sablier.trophees} periode={sablier.periode} date={sablier.date} aujourdhui={sablier.aujourdhui} />
      <section class="sab-sec" aria-label="Stats par domaine">
        <h2 class="sab-h2">{intitule(sablier.periode, sablier.date, sablier.aujourdhui)}, domaine par domaine</h2>
        <div class="sab-g sab-g2">
          {#each liste as c (c.domaine)}
            <SablierCarte carte={c} periode={sablier.periode} date={sablier.date} aujourdhui={sablier.aujourdhui} ailleurs={sablier.ailleurs[c.domaine] ?? null} onaller={(a) => sablier.aller(a.periode, a.date)} />
          {/each}
        </div>
      </section>
    {/if}
  </div>
</div>
{#if pret}<SablierAnnonce />{/if}
