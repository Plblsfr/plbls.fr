<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { euros, eurosSignes, pluriel } from './formats'
  import type { Resume } from './decision'
  import type { Recurrents } from './habitudes'
  import TirelireBref from './TirelireBref.svelte'
  import TirelireCourbe from './TirelireCourbe.svelte'
  import TirelireEcart from './TirelireEcart.svelte'
  import TirelireEtincelle from './TirelireEtincelle.svelte'
  import TirelireJauge from './TirelireJauge.svelte'
  import { ANNEAU, arcs, etatPeriode, eurosRonds, eurosRondsSignes, nomPrecedente, parRang, phraseProjection, pourcent, pourcentSigne, RAISONS, referenceDe, type Mois } from './stats'
  import { teinte } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { mois, recurrents = null, resume = null }: { mois: Mois; recurrents?: Recurrents | null; resume?: Resume | null } = $props()

  let en = $state<'euros' | 'pourcent'>('euros')

  const calendaire = $derived(mois.mode === 'calendar')
  const avant = $derived(nomPrecedente(mois.cumulative.previous?.id, mois.mode))
  const budget = $derived(mois.budget)
  const projection = $derived(mois.projection)
  const arc = $derived(arcs(budget?.bp ?? 0))
  const depassements = $derived(parRang(mois.overruns, en))
  const ligne = (id: number) => mois.categories.find((c) => c.id === id)
  const sous = $derived(mois.categories.filter((c) => c.cap_delta !== null && c.cap_delta < 0 && c.amount > 0))
  const sansRepere = $derived(!budget && mois.categories.every((c) => !c.avg3))
  const echelle = $derived(Math.max(projection?.total ?? 0, budget?.total ?? 0, mois.expenses.total, 1))
  const part = (v: number) => `${(Math.max(0, v) / echelle) * 100}%`
  const dansBudget = $derived(budget ? Math.min(mois.expenses.total, budget.total) : mois.expenses.total)
  const auDessus = $derived(budget ? Math.max(0, mois.expenses.total - budget.total) : 0)
  const aVenir = $derived(projection ? Math.max(0, projection.total - mois.expenses.total) : 0)
  const aVenirDansBudget = $derived(budget ? Math.max(0, Math.min(aVenir, budget.total - mois.expenses.total)) : aVenir)
  const lien = (id: number) => hrefs.tirelire('categorie', id, `periode=${mois.period.id}`)
  const tuiles = $derived([
    { nom: 'Revenus', valeur: euros(mois.income.total), ecart: mois.income.previous, serie: mois.history.map((h) => h.income), note: mois.income.unclassified > 0 ? `dont ${euros(mois.income.unclassified)} à classer` : '' },
    { nom: 'Épargne', valeur: euros(mois.savings.total), ecart: mois.savings.previous, serie: mois.history.map((h) => h.savings), note: '' },
    { nom: 'Reste à vivre', valeur: euros(mois.remaining.total), ecart: mois.remaining.previous, serie: mois.history.map((h) => h.remaining), note: '' },
  ])
</script>

<p class="tir-note">{etatPeriode(mois.period, mois.mode)}</p>

{#if tirelire.compteur && tirelire.aClasser > 0}
  <a class="tir-bandeau" href={hrefs.tirelire('a-classer')}>
    <span class="tir-pas tir-pas--rose"><Icon name="boite" size={18} /></span>
    <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">{pluriel(tirelire.aClasser, 'opération', 'opérations')} à classer</span><span class="tir-note">{euros(tirelire.compteur.debits)} de débits sans catégorie · {pluriel(tirelire.compteur.merchants, 'commerçant', 'commerçants')}</span></span>
    <Icon name="droite" size={18} />
  </a>
{/if}

<div class="tir-tb">
  <section class="tir-carte tir-tb-budget" aria-label="Budget {calendaire ? 'du mois' : 'du relevé'}">
    {#if budget}
      <div class="tir-ligne" style:gap="16px">
        <svg class="tir-anneau" width="124" height="124" viewBox="0 0 124 124" role="img" aria-label="{budget.bp === null ? 'Budget nul' : `${pourcent(budget.bp)} du budget consommé`}">
          <circle cx="62" cy="62" r="50" class="tir-anneau-fond" />
          <circle cx="62" cy="62" r="50" class="tir-anneau-plein" stroke-dasharray="{arc.plein} {ANNEAU}" />
          {#if arc.exces > 0}<circle cx="62" cy="62" r="50" class="tir-anneau-exces" stroke-dasharray="{arc.exces} {ANNEAU}" />{/if}
          <text x="62" y="62" class="tir-anneau-valeur">{budget.bp === null ? '—' : pourcent(budget.bp)}</text>
          <text x="62" y="80" class="tir-anneau-note">du budget</text>
        </svg>
        <div class="tir-col tir-grandit">
          <span class="tir-note">{calendaire ? 'Dépensé ce mois' : 'Dépensé sur ce relevé'}</span>
          <span class="tir-gros">{euros(mois.expenses.total)}</span>
          <span class="tir-note">Budget (somme des plafonds) : {euros(budget.total)}</span>
          {#if budget.delta > 0}
            <span class="tir-etq tir-etq--danger"><Icon name="alerte" size={14} />{eurosSignes(budget.delta)} au-dessus</span>
          {:else}
            <span class="tir-etq tir-etq--ok"><Icon name="ok" size={14} />{euros(-budget.delta)} de marge</span>
          {/if}
        </div>
      </div>
      {#if budget.uncapped > 0}<p class="tir-note">Dont {euros(budget.uncapped)} dans des catégories sans plafond ou à classer.</p>{/if}
    {:else}
      <div class="tir-col">
        <span class="tir-note">{calendaire ? 'Dépensé ce mois' : 'Dépensé sur ce relevé'}</span>
        <span class="tir-gros">{euros(mois.expenses.total)}</span>
        {#if mois.expenses.previous}<TirelireEcart ecart={mois.expenses.previous} texte={`${eurosSignes(mois.expenses.previous.delta)} vs ${avant}`} />{/if}
      </div>
      <a class="tir-invite" href={hrefs.tirelire('categories')}>
        <span class="tir-pas"><Icon name="cible" size={18} /></span>
        <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Fixe des plafonds</span><span class="tir-note">Aucune catégorie n’a de plafond : sans budget, pas d’anneau. Donne un plafond mensuel aux catégories que tu veux surveiller.</span></span>
        <Icon name="droite" size={18} />
      </a>
    {/if}
    {#if projection}
      <div class="tir-col tir-separe" role="group" aria-label="Projection de fin de mois">
        <div class="tir-ligne tir-ligne--ecart"><h2 class="tir-h3">Projection fin de mois</h2><span class="tir-fort tir-fixe">≈ {eurosRonds(projection.total)}</span></div>
        {#if projection.budget_delta !== null}
          {#if projection.budget_delta > 0}
            <span class="tir-ec tir-ec--mal"><span aria-hidden="true">▲</span>{eurosRondsSignes(projection.budget_delta)} vs budget à ce rythme</span>
          {:else}
            <span class="tir-ec tir-ec--bien"><span aria-hidden="true">▼</span>{eurosRonds(-projection.budget_delta)} sous le budget à ce rythme</span>
          {/if}
        {/if}
        <span class="tir-proj" aria-hidden="true">
          <span class="tir-proj-dans" style:width={part(dansBudget)}></span>
          {#if auDessus > 0}<span class="tir-proj-dessus" style:width={part(auDessus)}></span>{/if}
          {#if aVenirDansBudget > 0}<span class="tir-proj-venir" style:width={part(aVenirDansBudget)}></span>{/if}
          {#if aVenir - aVenirDansBudget > 0}<span class="tir-proj-venir tir-proj-venir--dessus" style:width={part(aVenir - aVenirDansBudget)}></span>{/if}
          {#if budget}<span class="tir-jg-repere" style:left={part(budget.total)}></span>{/if}
        </span>
        <ul class="tir-legende">
          <li><span class="tir-carre tir-carre--dans" aria-hidden="true"></span>Dépensé{budget ? ' dans le budget' : ''}</li>
          {#if auDessus > 0}<li><span class="tir-carre tir-carre--dessus" aria-hidden="true"></span>Dépassement</li>{/if}
          <li><span class="tir-carre tir-carre--venir" aria-hidden="true"></span>À venir ({pluriel(projection.remaining_days, 'jour', 'jours')})</li>
        </ul>
        <p class="tir-note">{phraseProjection(mois)}</p>
      </div>
    {:else if mois.projection_reason && RAISONS[mois.projection_reason]}
      <p class="tir-note tir-separe">{RAISONS[mois.projection_reason]}</p>
    {/if}
  </section>

  <section class="tir-g2 tir-tb-tuiles" aria-label="Revenus, épargne et reste à vivre">
    {#each tuiles as t (t.nom)}
      <div class="tir-carte tir-carte--serre">
        <div class="tir-ligne tir-ligne--ecart"><span class="tir-note">{t.nom}</span><TirelireEtincelle valeurs={t.serie} /></div>
        <span class="tir-val tir-fixe">{t.valeur}</span>
        {#if t.ecart}<span class="tir-sous"><TirelireEcart ecart={t.ecart} bon="hausse" /><span class="tir-note">vs {avant}</span></span>{:else}<span class="tir-note">pas de période précédente</span>{/if}
        {#if t.note}<span class="tir-note">{t.note}</span>{/if}
      </div>
    {/each}
    <div class="tir-carte tir-carte--serre">
      <div class="tir-ligne tir-ligne--ecart"><span class="tir-note">Taux d’épargne</span><TirelireEtincelle valeurs={mois.history.map((h) => h.savings_rate_bp ?? 0)} /></div>
      {#if mois.savings.rate_bp === null}
        <span class="tir-val tir-val--vide">—</span>
        <span class="tir-note">aucun revenu sur la période</span>
      {:else}
        <span class="tir-val tir-fixe">{pourcent(mois.savings.rate_bp, 1)}</span>
        <span class="tir-note">épargne ÷ revenus</span>
      {/if}
    </div>
  </section>

  {#if resume && resume.period.id === mois.period.id}<TirelireBref {resume} extrait />{/if}

  <div class="tir-tb-courbe"><TirelireCourbe {mois} /></div>

  <section class="tir-col tir-tb-depasse" style:gap="10px" aria-label="Ce qui dépasse">
    <div class="tir-ligne tir-ligne--ecart tir-ligne--plis">
      <h2 class="tir-h2">Ce qui dépasse</h2>
      {#if depassements.length}<span class="tir-note">{pluriel(depassements.length, 'catégorie', 'catégories')} · {eurosSignes(mois.overrun_total)}</span>{/if}
    </div>
    {#if depassements.length}
      {#if depassements.length > 1}
        <div class="tir-seg" role="group" aria-label="Classement des dépassements">
          <button type="button" aria-pressed={en === 'euros'} onclick={() => (en = 'euros')}>Classé en €</button>
          <button type="button" aria-pressed={en === 'pourcent'} onclick={() => (en = 'pourcent')}>Classé en %</button>
        </div>
      {/if}
      <ol class="tir-col" style:gap="8px">
        {#each depassements as d, i (d.category_id)}
          {@const c = ligne(d.category_id)}
          {#if c}
            <li>
              <a class="tir-depasse" class:tir-depasse--premier={i === 0} href={lien(c.id)}>
                <span class="tir-pas" aria-hidden="true">{i + 1}</span>
                <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3"><span class="tir-sr">{i + 1}. </span>{c.name}</span><span class="tir-note">{euros(c.amount)} · {referenceDe(d, mois.mode)}</span></span>
                <span class="tir-depasse-courbe"><TirelireEtincelle valeurs={c.trend} /></span>
                <span class="tir-col tir-col--serre" style:align-items="flex-end"><span class="tir-ec tir-ec--mal tir-ec--grand"><span aria-hidden="true">▲</span>{eurosSignes(d.excess)}</span><span class="tir-etq tir-etq--danger">{d.excess_bp === null ? 'au-dessus' : pourcentSigne(d.excess_bp)}</span></span>
                <span class="tir-depasse-jauge"><TirelireJauge montant={c.amount} reference={d.reference_amount} /></span>
              </a>
            </li>
          {/if}
        {/each}
      </ol>
    {:else}
      <div class="tir-carte tir-carte--creux">
        {#if sansRepere}
          <p class="tir-note tir-note--texte">Rien à comparer pour l’instant : aucune catégorie n’a de plafond et il n’y a pas encore de période complète avant celle-ci pour calculer une moyenne.</p>
          <a class="tir-btn tir-btn--large" href={hrefs.tirelire('categories')}><Icon name="cible" size={16} />Fixer des plafonds</a>
        {:else}
          <p class="tir-note tir-note--texte"><Icon name="ok" size={16} /> Aucune catégorie ne dépasse son plafond ni sa moyenne habituelle sur cette période.</p>
        {/if}
      </div>
    {/if}
    {#if sous.length}
      <div class="tir-ligne tir-ligne--plis"><span class="tir-note">Sous le plafond :</span>{#each sous as c (c.id)}<a class="tir-puce-lien" href={lien(c.id)}><span class="tir-etq tir-etq--ok"><span class="tir-pt tir-pt--{teinte(c)}" aria-hidden="true"></span>{c.name} {eurosSignes(c.cap_delta ?? 0)}</span></a>{/each}</div>
    {/if}
    <a class="tir-btn tir-btn--large" href={hrefs.tirelire('mois', mois.period.id)}>Voir {calendaire ? 'le mois' : 'le relevé'} en détail<Icon name="droite" size={16} /></a>
  </section>

  {#if recurrents}
    <a class="tir-bandeau tir-tb-abonnements" class:tir-bandeau--calme={!recurrents.alerts_unseen} href={hrefs.tirelire('habitudes', 'abonnements')} aria-label="Abonnements et prélèvements{recurrents.alerts_unseen ? ` : ${pluriel(recurrents.alerts_unseen, 'alerte non vue', 'alertes non vues')}` : ''}">
      <span class="tir-pas" class:tir-pas--rose={!recurrents.alerts_unseen}><Icon name="actualise" size={18} /></span>
      <span class="tir-col tir-col--colle tir-grandit">
        <span class="tir-h3">Abonnements et prélèvements</span>
        <span class="tir-note">{#if recurrents.total.count}{euros(recurrents.total.monthly)} par mois · {pluriel(recurrents.total.count, 'récurrent', 'récurrents')}{:else}aucun récurrent détecté pour l’instant{/if}</span>
      </span>
      {#if recurrents.alerts_unseen}<span class="tir-etq tir-etq--fort">{pluriel(recurrents.alerts_unseen, 'alerte', 'alertes')}</span>{/if}
      <Icon name="droite" size={18} />
    </a>
  {/if}
  <a class="tir-bandeau tir-bandeau--calme tir-tb-simulateur" href={hrefs.tirelire('decision')}>
    <span class="tir-pas tir-pas--rose"><Icon name="curseurs" size={18} /></span>
    <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Simulateur</span><span class="tir-note">si je baisse une catégorie, combien j’économise par mois et par an</span></span>
    <Icon name="droite" size={18} />
  </a>
</div>
