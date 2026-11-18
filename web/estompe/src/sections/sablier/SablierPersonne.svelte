<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { nomPersonne } from './admin'
  import { admin } from './admin.svelte'
  import { confirmerEffacementDe } from './effacement'
  import { dateHeure, jourLocal, jourLong } from './formats'
  import { intitule, libelleRecap } from './periodes'
  import { lecturesDe, sablierApi } from './sablier'
  import SablierCarte from './SablierCarte.svelte'
  import SablierErreur from './SablierErreur.svelte'
  import SablierPeriode from './SablierPeriode.svelte'
  import SablierRecap from './SablierRecap.svelte'
  import SablierSeries from './SablierSeries.svelte'
  import SablierTrophees from './SablierTrophees.svelte'
  import { EtatSablier } from './sablier.svelte'
  import { cartes, SALON_INACCESSIBLE, type NomSalon } from './stats'

  let { sub }: { sub: string } = $props()
  // svelte-ignore state_referenced_locally
  const etat = new EtatSablier({ ...sablierApi, ...lecturesDe(sub) }, jourLocal, true)
  let consultee = $state('')
  let recap = $state<number | null>(null)
  let effacement = $state(false)

  async function ouvrir() {
    await etat.ouvrir()
    if (etat.charge) { consultee = new Date().toISOString(); void etat.chargerRecaps() }
  }
  onMount(() => {
    if (!admin.personnes) void admin.charger()
    void ouvrir()
  })

  const nomSalon: NomSalon = (serveur, salon) => app.details[serveur]?.channels.find((c) => c.id === salon)?.name ?? null
  const personne = $derived(admin.trouver(sub))
  const nom = $derived(personne ? nomPersonne(personne) : nomPersonne({ sub, name: null, known: true }))
  const moi = $derived(sub === app.sablierMe?.sub)
  const pret = $derived(etat.charge && !!etat.stats && !!etat.trophees && !!etat.series)
  const liste = $derived(etat.stats ? cartes(etat.stats, nomSalon, etat.aujourdhui, SALON_INACCESSIBLE) : [])

  async function effacer() {
    if (effacement || !(await confirmerEffacementDe(nom))) return
    effacement = true
    try {
      await admin.effacer(sub)
      notify.ok(`Les données de ${nom} sont effacées.`)
      router.go(hrefs.stats('admin', 'personnes'))
    } catch (e) {
      notify.error(e, 'Les données n’ont pas pu être effacées')
    } finally {
      effacement = false
    }
  }
</script>

<section class="sab-sec" aria-label="Stats de {nom}">
  <div class="sab-ligne"><a class="plbls-button plbls-button--contour plbls-button--compact" href={hrefs.stats('admin', 'personnes')}><Icon name="gauche" size={16} />Personnes</a></div>
  <div class="sab-col" style:gap="0">
    <h2 class="sab-h2">{nom}</h2>
    {#if etat.premierJour}<p class="sab-note">Premier jour d’activité : {jourLong(etat.premierJour)}</p>{/if}
  </div>
  {#if !pret && etat.erreur}
    <SablierErreur message={etat.erreur} onretry={() => void ouvrir()} />
  {:else if !pret}
    <div class="sab-attente"><Loader text="Ouverture des stats…" /></div>
  {:else if etat.stats && etat.trophees && etat.series}
    {#if moi}
      <div class="sab-alerte" role="note"><Icon name="oeil" size={18} /><p>Ce sont tes propres stats : cette consultation n’est pas notée au journal.</p></div>
    {:else if consultee}
      <div class="sab-alerte" role="status"><Icon name="oeil" size={18} /><p>Consultation notée au journal le {dateHeure(consultee)}. {nom} la voit dans « Partage et données ».</p></div>
    {/if}
    <SablierPeriode {etat} />
    {#if etat.erreur}
      <div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{etat.erreur}</p><button class="plbls-button plbls-button--compact plbls-button--contour" type="button" onclick={() => void etat.chargerStats()}>Réessayer</button></div>
    {/if}
    <p class="sab-note">{intitule(etat.periode, etat.date, etat.aujourdhui)} · mêmes stats que celles que {nom} voit</p>
    <div class="sab-g sab-g2">
      {#each liste as c (c.domaine)}<SablierCarte carte={c} periode={etat.periode} date={etat.date} aujourdhui={etat.aujourdhui} />{/each}
    </div>
    <SablierSeries series={etat.series} />
    <SablierTrophees trophees={etat.trophees} periode={etat.periode} date={etat.date} aujourdhui={etat.aujourdhui} />
    {#if etat.recaps?.length}
      <section class="sab-sec" aria-label="Récaps">
        <h2 class="sab-h2">Récaps</h2>
        <ul class="sab-ligne">
          {#each etat.recaps as r (r.id)}<li><button class="plbls-button plbls-button--contour plbls-button--compact" type="button" onclick={() => (recap = r.id)} aria-label="Regarder le récap {libelleRecap(r.kind, r.period)}">{libelleRecap(r.kind, r.period)}</button></li>{/each}
        </ul>
      </section>
    {/if}
  {/if}
  <div class="sab-card">
    <div class="sab-tete" style:align-items="center">
      <div class="sab-col"><p class="sab-h3">Effacer les données de {nom}</p><p class="sab-note">Stats, trophées, récaps et réglages, définitivement. La collecte continue ensuite. Un import d’historique rejoué après l’effacement serait compté de nouveau.</p></div>
      <button class="plbls-button plbls-button--danger" type="button" disabled={effacement} onclick={effacer}>Effacer ses données…</button>
    </div>
  </div>
</section>
{#if recap !== null}{#key recap}<SablierRecap id={recap} {etat} onclose={() => (recap = null)} />{/key}{/if}
