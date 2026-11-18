<script lang="ts">
  import { onMount } from 'svelte'
  import Loader from '../../components/Loader.svelte'
  import { messageDe } from './admin.svelte'
  import { duree, jourCourt, nombre } from './formats'
  import { intitule } from './periodes'
  import { adminApi, type Totaux } from './sablier'
  import SablierErreur from './SablierErreur.svelte'
  import SablierPeriode from './SablierPeriode.svelte'
  import { NavigationPeriode } from './sablier.svelte'
  import { barresParJour } from './stats'

  let totaux = $state.raw<Totaux | null>(null)
  let erreur = $state('')
  let tour = 0

  async function charger() {
    const moi = ++tour
    erreur = ''
    try {
      const t = await adminApi.totaux(etat.periode, etat.date)
      if (moi === tour) totaux = t
    } catch (e) {
      if (moi === tour) erreur = messageDe(e)
    }
  }
  const etat = new NavigationPeriode(undefined, () => void charger())
  onMount(() => {
    void charger()
    adminApi.totaux('all', etat.aujourdhui).then((t) => (etat.premierJour = t.from), () => {})
  })

  const tuiles = $derived(totaux ? [
    { label: totaux.active_people >= 2 ? 'Personnes actives' : 'Personne active', n: totaux.active_people, valeur: nombre(totaux.active_people) },
    { label: 'Temps actif', n: totaux.usage_seconds, valeur: duree(totaux.usage_seconds) },
    { label: 'Temps regardé', n: totaux.watched_seconds, valeur: duree(totaux.watched_seconds) },
    { label: 'Temps d’écoute', n: totaux.listened_seconds, valeur: duree(totaux.listened_seconds) },
    { label: totaux.messages >= 2 ? 'Messages envoyés' : 'Message envoyé', n: totaux.messages, valeur: nombre(totaux.messages) },
  ].filter((t) => t.n > 0) : [])
  const barres = $derived(totaux ? barresParJour({ period: totaux.period, from: totaux.from, to: totaux.to, by_day: totaux.by_day.map((j) => ({ day: j.day, seconds: j.usage_seconds })) }, etat.aujourdhui) : [])
</script>

<section class="sab-sec" aria-label="Totaux globaux">
  <SablierPeriode {etat} />
  {#if erreur && !totaux}
    <SablierErreur message={erreur} onretry={() => void charger()} />
  {:else if !totaux}
    <div class="sab-attente"><Loader text="Calcul des totaux…" /></div>
  {:else}
    {#if erreur}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{erreur}</p><button class="plbls-button plbls-button--compact plbls-button--contour" type="button" onclick={() => void charger()}>Réessayer</button></div>{/if}
    <p class="sab-note">{intitule(etat.periode, etat.date, etat.aujourdhui)} · tout plbls.fr, sans classement entre personnes</p>
    {#if tuiles.length}
      <div class="sab-g sab-g4">
        {#each tuiles as t (t.label)}<div class="sab-card"><div class="sab-tile"><span>{t.label}</span><b>{t.valeur}</b></div></div>{/each}
      </div>
      {#if barres.length && totaux.by_day.length >= 2}
        <div class="sab-card">
          <p class="sab-h3">Temps actif par jour</p>
          <div class="sab-barres" role="img" aria-label="Temps actif de tout le monde par jour, du {jourCourt(totaux.from)} au {jourCourt(totaux.to > etat.aujourdhui ? etat.aujourdhui : totaux.to)}">
            {#each barres as b, i (i)}<i class:off={!b.actif} style:height="{b.hauteur}%"></i>{/each}
          </div>
        </div>
      {/if}
    {:else}
      <div class="sab-vide"><h2 class="sab-h2">Aucune activité sur cette période</h2></div>
    {/if}
  {/if}
</section>
