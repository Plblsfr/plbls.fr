<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { app } from '../../lib/store.svelte'
  import { dateCourte, euros, periode, pluriel } from './formats'
  import { estOutil, ONGLETS, ongletActif, periodeDe, periodeValide, voletDe, vueDe } from './onglets'
  import { titrePeriode } from './stats'
  import TirelireAccueil from './TirelireAccueil.svelte'
  import TirelireCategorie from './TirelireCategorie.svelte'
  import TirelireCategories from './TirelireCategories.svelte'
  import TirelireDecision from './TirelireDecision.svelte'
  import { prefillDe } from './simulation'
  import TirelireEvolution from './TirelireEvolution.svelte'
  import TirelireClasser from './TirelireClasser.svelte'
  import TirelireAbonnements from './TirelireAbonnements.svelte'
  import TirelireHabitudes from './TirelireHabitudes.svelte'
  import TirelireImport from './TirelireImport.svelte'
  import TirelireMois from './TirelireMois.svelte'
  import TirelireOnglets from './TirelireOnglets.svelte'
  import TirelireOperation from './TirelireOperation.svelte'
  import TirelireOperations from './TirelireOperations.svelte'
  import TirelireTete from './TirelireTete.svelte'
  import TirelireVide from './TirelireVide.svelte'
  import { NATURES_LONGUES, natureDe, nomReleve } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { section, id, requete, onmenu }: { section?: string; id?: string; requete?: string; onmenu: () => void } = $props()

  const vue = $derived(vueDe(section, id))
  const actif = $derived(ongletActif(vue))
  const nom = $derived(app.tirelireMe?.name ?? '')
  const vide = $derived(tirelire.releves?.length === 0)
  const operation = $derived(vue === 'operation' ? tirelire.operation(Number(id)) : null)
  const filtre = $derived(tirelire.filtres.releve === null ? null : tirelire.releve(tirelire.filtres.releve))
  const portee = $derived.by(() => {
    if (tirelire.filtres.du && tirelire.filtres.au) return periode(tirelire.filtres.du, tirelire.filtres.au).toLowerCase()
    if (tirelire.filtres.du) return `depuis le ${dateCourte(tirelire.filtres.du)}`
    if (tirelire.filtres.au) return `jusqu’au ${dateCourte(tirelire.filtres.au)}`
    if (filtre) return nomReleve(filtre).toLowerCase()
    return tirelire.releves?.length ? pluriel(tirelire.releves.length, 'relevé', 'relevés') : ''
  })
  const cible = $derived(vue === 'mois' ? periodeValide(id) : vue === 'categorie' || vue === 'decision' ? periodeValide(periodeDe(requete)) : undefined)
  const mois = $derived(tirelire.mois)
  const aJour = $derived(!!mois && (cible ? mois.period.id === cible : mois.next_id === null))
  const detail = $derived(vue === 'categorie' && tirelire.categorieDuMois?.category.id === Number(id) ? tirelire.categorieDuMois : null)
  const titre = $derived(vue === 'accueil' ? (vide ? 'Bienvenue' : mois && aJour ? titrePeriode(mois.period, mois.mode) : 'Accueil') : (ONGLETS.find((o) => o.id === actif)?.long ?? 'Tirelire'))

  function charger() {
    if (vue === 'habitudes' || vue === 'abonnements' || vue === 'accueil') void tirelire.chargerRecurrents()
    if (vue === 'abonnements') return
    if (vue === 'accueil' || vue === 'mois' || vue === 'decision') void tirelire.chargerResume(cible)
    if (vue === 'decision') return void tirelire.chargerSimulation()
    if (vue === 'habitudes') void tirelire.chargerHabitudes()
    else if (vue === 'evolution') void tirelire.chargerEvolution()
    else if (vue === 'categorie') void tirelire.chargerCategorieDuMois(Number(id), cible)
    else void tirelire.chargerMois(cible)
  }

  $effect(() => {
    const pret = !!tirelire.releves?.length && (vue === 'accueil' || vue === 'mois' || vue === 'categorie' || vue === 'evolution' || vue === 'habitudes' || vue === 'abonnements' || vue === 'decision')
    const cle = [vue, id, cible, tirelire.mode, tirelire.version, vue === 'evolution' ? tirelire.fenetre : vue === 'habitudes' ? tirelire.fenetreHabitudes : ''].join()
    untrack(() => {
      if (!pret || !cle) return
      charger()
      if (vue === 'mois' && !tirelire.periodes) void tirelire.chargerPeriodes()
    })
  })

  onMount(() => { void tirelire.ouvrir() })
</script>

<div class="tir" data-vue={vue}>
  {#if vue === 'import'}
    <TirelireTete titre="Import" sous="relevés de compte au format PDF" retour={hrefs.tirelire()} retourLabel="Retour à l’accueil" />
  {:else if vue === 'classer'}
    <TirelireTete titre="À classer" sous={tirelire.compteur ? (tirelire.aClasser ? `${pluriel(tirelire.aClasser, 'opération restante', 'opérations restantes')} · ${euros(tirelire.compteur.debits)} de débits` : 'rien à classer') : ''} retour={hrefs.tirelire()} retourLabel="Retour à l’accueil">
      <a class="tir-ib" href={hrefs.tirelire('categories')} aria-label="Catégories et règles" title="Catégories et règles"><Icon name="curseurs" size={20} /></a>
    </TirelireTete>
  {:else if vue === 'categories'}
    <TirelireTete titre="Catégories et règles" sous={tirelire.categories ? [pluriel(tirelire.categories.length, 'catégorie', 'catégories'), tirelire.regles ? pluriel(tirelire.regles.length, 'règle', 'règles') : '', tirelire.commercants ? pluriel(tirelire.commercants.length, 'commerçant', 'commerçants') : ''].filter(Boolean).join(' · ') : ''} retour={hrefs.tirelire()} retourLabel="Retour à l’accueil" />
  {:else if vue === 'categorie'}
    <TirelireTete titre={detail?.category.name ?? 'Catégorie'} sous={detail ? `${NATURES_LONGUES[detail.category.kind].toLowerCase()} · ${titrePeriode(detail.period, detail.mode).toLowerCase()}` : ''} retour={hrefs.tirelire('mois', detail?.period.id ?? cible)} retourLabel="Retour à la vue du mois">
      <a class="tir-ib" href={hrefs.tirelire('categories')} aria-label="Catégories et règles" title="Catégories et règles"><Icon name="curseurs" size={20} /></a>
    </TirelireTete>
  {:else if vue === 'decision'}
    <TirelireTete titre="Aide à la décision" sous="résumé de la période et simulateur" retour={hrefs.tirelire()} retourLabel="Retour à l’accueil" />
  {:else if vue === 'abonnements'}
    <TirelireTete titre="Abonnements" sous="et paiements récurrents" retour={hrefs.tirelire('habitudes')} retourLabel="Retour aux habitudes" />
  {:else if vue === 'operation'}
    <TirelireTete titre="Opération" sous={operation ? `${dateCourte(operation.date)} · ${natureDe(operation)}` : ''} retour={hrefs.tirelire('operations')} retourLabel="Retour aux opérations" />
  {:else}
    <TirelireTete {titre} sur={['Tirelire', vue === 'operations' ? portee : nom].filter(Boolean).join(' · ')} {onmenu}>
      {#if tirelire.aClasser}<a class="tir-ib tir-ib--pip" href={hrefs.tirelire('a-classer')} aria-label="À classer : {pluriel(tirelire.aClasser, 'opération', 'opérations')}" title="À classer"><Icon name="boite" size={20} /><span class="tir-pip" aria-hidden="true">{tirelire.aClasser > 99 ? '99+' : tirelire.aClasser}</span></a>{/if}
      <a class="tir-ib" href={hrefs.tirelire('import')} aria-label="Importer des relevés" title="Importer des relevés"><Icon name="envoie" size={20} /></a>
    </TirelireTete>
  {/if}

  <div class="tir-defile">
    <div class="tir-corps">
      {#if vue === 'import'}
        <TirelireImport />
      {:else if vue === 'classer'}
        <TirelireClasser />
      {:else if vue === 'categories'}
        <TirelireCategories volet={voletDe(id)} />
      {:else if vue === 'operation'}
        {#key id}<TirelireOperation id={Number(id)} />{/key}
      {:else if vue === 'operations'}
        <TirelireOperations {requete} />
      {:else if tirelire.releves && vide && vue === 'accueil'}
        <TirelireVide />
      {:else if tirelire.releves && vide}
        <section class="tir-carte tir-etat" aria-label="Aucun relevé">
          <span class="tir-pas tir-pas--grand tir-pas--rose"><Icon name="envoie" size={22} /></span>
          <h2 class="tir-h2">Aucun relevé importé</h2>
          <p class="tir-note tir-note--texte">Les statistiques se calculent à partir de tes relevés.</p>
          <a class="tir-btn tir-btn--plein" href={hrefs.tirelire('import')}>Importer un relevé PDF</a>
        </section>
      {:else if tirelire.releves && vue === 'decision' && (tirelire.simulation || tirelire.erreurSimulation)}
        {#key JSON.stringify(prefillDe(requete))}<TirelireDecision resume={tirelire.resume} simulation={tirelire.simulation} {requete} />{/key}
      {:else if tirelire.releves && vue === 'decision'}
        <div class="tir-etat" role="status"><Loader text="Calcul du résumé et de la base du simulateur…" /></div>
      {:else if tirelire.releves && vue === 'abonnements' && tirelire.recurrents}
        <TirelireAbonnements recurrents={tirelire.recurrents} />
      {:else if tirelire.releves && (vue === 'abonnements' ? tirelire.erreurRecurrents : vue === 'habitudes' && tirelire.erreurHabitudes)}
        <section class="tir-carte tir-etat" role="alert">
          <span class="tir-pas tir-pas--grand tir-pas--danger"><Icon name="alerte" size={22} /></span>
          <h2 class="tir-h2">{vue === 'abonnements' ? 'Abonnements indisponibles' : 'Habitudes indisponibles'}</h2>
          <p class="tir-note tir-note--texte">{vue === 'abonnements' ? tirelire.erreurRecurrents : tirelire.erreurHabitudes}</p>
          <button class="tir-btn tir-btn--plein" onclick={charger}><Icon name="actualise" size={16} />Réessayer</button>
        </section>
      {:else if tirelire.releves && vue === 'habitudes' && tirelire.habitudes}
        <TirelireHabitudes habitudes={tirelire.habitudes} recurrents={tirelire.recurrents} />
      {:else if tirelire.releves && (vue === 'habitudes' || vue === 'abonnements')}
        <div class="tir-etat" role="status"><Loader text={vue === 'abonnements' ? 'Recherche des paiements récurrents…' : 'Calcul des habitudes…'} /></div>
      {:else if tirelire.releves && vue === 'evolution' && tirelire.erreurEvolution}
        <section class="tir-carte tir-etat" role="alert">
          <span class="tir-pas tir-pas--grand tir-pas--danger"><Icon name="alerte" size={22} /></span>
          <h2 class="tir-h2">Évolution indisponible</h2>
          <p class="tir-note tir-note--texte">{tirelire.erreurEvolution}</p>
          <button class="tir-btn tir-btn--plein" onclick={charger}><Icon name="actualise" size={16} />Réessayer</button>
        </section>
      {:else if tirelire.releves && vue === 'evolution' && tirelire.evolution}
        <TirelireEvolution evolution={tirelire.evolution} />
      {:else if tirelire.releves && vue === 'evolution'}
        <div class="tir-etat" role="status"><Loader text="Calcul de l’évolution…" /></div>
      {:else if tirelire.releves && tirelire.erreurMois}
        <section class="tir-carte tir-etat" role="alert">
          <span class="tir-pas tir-pas--grand tir-pas--danger"><Icon name="alerte" size={22} /></span>
          <h2 class="tir-h2">Statistiques indisponibles</h2>
          <p class="tir-note tir-note--texte">{tirelire.erreurMois}</p>
          <button class="tir-btn tir-btn--plein" onclick={charger}><Icon name="actualise" size={16} />Réessayer</button>
        </section>
      {:else if tirelire.releves && (vue === 'categorie' ? tirelire.categorieAbsente : tirelire.moisAbsent)}
        <section class="tir-carte tir-etat" aria-label="Introuvable">
          <span class="tir-pas tir-pas--grand"><Icon name="calendar" size={22} /></span>
          <h2 class="tir-h2">{vue === 'categorie' ? 'Catégorie ou période introuvable' : 'Période introuvable'}</h2>
          <p class="tir-note tir-note--texte">Aucun relevé importé ne couvre cette période{vue === 'categorie' ? ', ou cette catégorie n’existe plus' : ''}.</p>
          <a class="tir-btn tir-btn--plein" href={hrefs.tirelire('mois')}>Voir la période la plus récente</a>
        </section>
      {:else if tirelire.releves && vue === 'categorie' && detail}
        <TirelireCategorie {detail} />
      {:else if tirelire.releves && vue === 'accueil' && mois && aJour}
        <TirelireAccueil {mois} recurrents={tirelire.recurrents} resume={tirelire.resume} />
      {:else if tirelire.releves && vue === 'mois' && mois}
        <div class="tir-col" style:gap="12px" style:opacity={aJour ? undefined : 0.6} aria-busy={!aJour}><TirelireMois {mois} resume={tirelire.resume} /></div>
      {:else if tirelire.releves}
        <div class="tir-etat" role="status"><Loader text="Calcul des statistiques…" /></div>
      {:else if tirelire.erreur}
        <section class="tir-carte tir-etat" role="alert">
          <span class="tir-pas tir-pas--grand tir-pas--danger"><Icon name="alerte" size={22} /></span>
          <h2 class="tir-h2">Tirelire ne répond pas</h2>
          <p class="tir-note tir-note--texte">{tirelire.erreur} Le reste de l’app fonctionne normalement.</p>
          <button class="tir-btn tir-btn--plein" onclick={() => void tirelire.chargerReleves()}><Icon name="actualise" size={16} />Réessayer</button>
        </section>
      {:else}
        <div class="tir-etat" role="status"><Loader text="Chargement de Tirelire…" /></div>
      {/if}
    </div>
  </div>

  {#if !estOutil(vue)}<TirelireOnglets {actif} />{/if}
</div>
